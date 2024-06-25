SELECT
  a,
  b,
  d[1],
  d.1,
  'c',
  - e,
  d.*,
  sum(e + f) / 100 AS g
FROM (
  SELECT
    id
  FROM table1) AS subquery1_alias
JOIN (
  SELECT
    *
  FROM table3) AS subquery2_alias
  USING
    a,
    b
JOIN db1.table5 AS table5_alias
  ON
    a = b
JOIN table7 AS table7_alias
  USING
    c
WHERE
  (a = b) AND TRUE AND 1 = 1
GROUP BY
  (a + b) AS c,
  d
HAVING
  a AND b
ORDER BY
  a,
  b DESC
LIMIT 100
;
