use std::io::IsTerminal;
use std::str::FromStr;

use clap::Parser;
use tracing::*;

use sqlsonnet::Queries;

lazy_static::lazy_static! {
    static ref THEMES: Vec<String> =
        bat::assets::HighlightingAssets::from_binary().themes().map(String::from).collect();
}

#[derive(Debug, miette::Diagnostic, thiserror::Error)]
enum Error {
    #[error("Failed to read input")]
    Input(#[from] clap_stdin::StdinError),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Inner(#[from] sqlsonnet::Error),
    #[error("Failed to highlight SQL")]
    Bat(#[from] bat::error::Error),
    #[error(transparent)]
    Miette(#[from] miette::InstallError),
}

#[derive(Parser)]
#[clap(version)]
struct Flags {
    /// Color theme for syntax highlighting
    #[clap(long, env = "SQLSONNET_THEME",
           value_parser=clap::builder::PossibleValuesParser::new(THEMES.iter().map(|s| s.as_str())))]
    theme: Option<String>,
    /// Compact SQL representation
    #[clap(long, short)]
    compact: bool,
    /// Input file (path or - for stdin).
    input: Input,
    /// Convert an SQL file into Jsonnet.
    #[clap(long, short)]
    from_sql: bool,
    /// With --from-sql: Convert back to SQL and print the differences with the original, if any.
    #[clap(long, requires = "from_sql")]
    diff: bool,
    #[clap(long, value_delimiter = ',')]
    display_format: Option<Vec<Language>>,
}

#[derive(Clone)]
struct Input(clap_stdin::FileOrStdin);
impl FromStr for Input {
    type Err = <clap_stdin::FileOrStdin as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        clap_stdin::FileOrStdin::from_str(s).map(Self)
    }
}

impl Input {
    fn contents(&self) -> Result<String, clap_stdin::StdinError> {
        self.0.clone().contents()
    }
    fn filename(&self) -> String {
        match &self.0.source {
            clap_stdin::Source::Stdin => "<stdin>".into(),
            clap_stdin::Source::Arg(s) => s.clone(),
        }
    }
    fn resolver(&self) -> sqlsonnet::FsResolver {
        match &self.0.source {
            clap_stdin::Source::Stdin => Default::default(),
            clap_stdin::Source::Arg(s) => sqlsonnet::FsResolver::from_filename(s),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
enum Language {
    Sql,
    Jsonnet,
    Json,
}
impl Language {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Sql => "sql",
            Self::Json => "json",
            Self::Jsonnet => "jsonnet",
        }
    }
}

fn highlight<T: std::fmt::Display>(
    snippet: T,
    language: Language,
    args: &Flags,
) -> Result<(), Error> {
    let is_tty = std::io::stdout().is_terminal();
    if is_tty {
        let mut printer = bat::PrettyPrinter::new();
        if let Some(theme) = &args.theme {
            printer.theme(theme);
        }
        let sql = std::io::Cursor::new(snippet.to_string());
        printer
            .input(bat::Input::from_reader(sql).name(format!("queries.{}", language.as_str())))
            .language(language.as_str())
            .header(true);

        printer.print()?;
    } else {
        println!("{}", snippet);
    }
    println!();
    Ok(())
}

fn main() -> miette::Result<()> {
    Ok(main_impl()?)
}

fn main_impl() -> Result<(), Error> {
    let start = std::time::Instant::now();
    sqlsonnet::setup_logging();
    let args = Flags::parse();

    let assets = bat::assets::HighlightingAssets::from_binary();
    let theme = assets.get_theme(
        args.theme
            .as_deref()
            .unwrap_or_else(|| bat::assets::HighlightingAssets::default_theme()),
    );
    let highlighter = miette::highlighters::SyntectHighlighter::new(
        assets.get_syntax_set().unwrap().clone(),
        theme.clone(),
        false,
    );
    miette::set_hook(Box::new(move |_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .context_lines(10)
                .with_syntax_highlighting(highlighter.clone())
                .build(),
        )
    }))?;

    let display_format = args.display_format.clone().unwrap_or_else(|| {
        vec![if args.from_sql {
            Language::Jsonnet
        } else {
            Language::Sql
        }]
    });
    let filename = args.input.filename();
    let input = args.input.contents()?;
    if args.from_sql {
        info!("Converting SQL file {}", filename);
        let queries = Queries::from_sql(&input)?;
        let has = |l| display_format.iter().any(|l2| l2 == &l);
        let sql = queries.to_sql(args.compact);
        if has(Language::Sql) {
            highlight(&sql, Language::Sql, &args)?;
        }
        if has(Language::Jsonnet) {
            let jsonnet = queries.as_jsonnet();
            highlight(jsonnet, Language::Jsonnet, &args)?;
        }
        if args.diff && input != sql {
            println!("{}", pretty_assertions::StrComparison::new(&input, &sql));
        }
    } else {
        let contents = sqlsonnet::import_utils() + &input;
        info!("Converting Jsonnet file {} to SQL", filename);

        // TODO: Support passing a single query.
        let queries = Queries::from_jsonnet(&contents, args.input.resolver())?;

        let has = |l| display_format.iter().any(|l2| l2 == &l);
        // Display queries
        debug!("{:#?}", queries);
        if has(Language::Jsonnet) {
            highlight(&contents, Language::Jsonnet, &args)?;
        }
        if has(Language::Sql) {
            highlight(queries.to_sql(args.compact), Language::Sql, &args)?;
        }
    }

    info!(elapsed=?start.elapsed(), "Done");
    Ok(())
}
