# Reads `<case> <engine> <run> <json>` lines and prints the best-of-N table.

function field(text, key,   pattern, piece) {
  pattern = "\"" key "\":"
  piece = text
  if (index(piece, pattern) == 0) return ""
  piece = substr(piece, index(piece, pattern) + length(pattern))
  gsub(/^"/, "", piece)
  sub(/[,}].*$/, "", piece)
  gsub(/"/, "", piece)
  return piece
}

{
  json = ""
  for (part = 4; part <= NF; part++) json = json (part > 4 ? " " : "") $part
  case_name = $1
  engine = $2
  event = field(json, "event")
  key = case_name "|" engine

  if (event == "loaded") {
    value = field(json, "us")
    if (value == "") value = field(json, "ms") * 1000
    value = value + 0
    if (!(key in load_us) || value < load_us[key]) load_us[key] = value
    value = field(json, "intern_us") + 0
    if (!(key in intern_us) || value < intern_us[key]) intern_us[key] = value
    strings[key] = field(json, "strings")
    nodes[key] = field(json, "nodes")
    edges[key] = field(json, "edges")
  } else if (event == "fixpoint") {
    value = field(json, "us")
    if (value == "") value = field(json, "ms") * 1000
    value = value + 0
    if (!(key in fixpoint_us) || value < fixpoint_us[key]) fixpoint_us[key] = value
    derived[key] = field(json, "derived")
    if (!(key in seen)) { seen[key] = 1; order[++count] = key }
  } else if (event == "materialize") {
    value = field(json, "us") + 0
    if (!(key in material_us) || value < material_us[key]) material_us[key] = value
  } else if (event == "done") {
    checksum[key] = field(json, "checksum")
    rss[key] = field(json, "peak_rss_kb")
  } else if (event == "insert") {
    source = field(json, "source")
    variant = field(json, "variant")
    insert_rows[source "|" variant] = field(json, "rows")
    insert_us[source "|" variant] = field(json, "us")
    insert_rate[source "|" variant] = field(json, "rows_per_sec")
    insert_stored[source "|" variant] = field(json, "stored")
    if (!(source "|" variant in insert_seen)) {
      insert_seen[source "|" variant] = 1
      insert_order[++insert_count] = source "|" variant
    }
  }
}

END {
  print "| case | engine | derived | checksum | load us | intern us | fixpoint ms | fp rows/sec | materialize ms | intern % of total | peak rss kb |"
  print "|---|---|---|---|---|---|---|---|---|---|---|"
  for (row = 1; row <= count; row++) {
    key = order[row]
    split(key, parts, "|")
    total = load_us[key] + fixpoint_us[key] + material_us[key]
    rate = fixpoint_us[key] > 0 ? int(derived[key] * 1000000 / fixpoint_us[key]) : 0
    share = total > 0 ? intern_us[key] * 100.0 / total : 0
    printf "| %s | %s | %s | %s | %d | %d | %.0f | %d | %.0f | %.3f%% | %s |\n",
      parts[1], parts[2], derived[key], checksum[key], load_us[key],
      intern_us[key], fixpoint_us[key] / 1000.0, rate,
      material_us[key] / 1000.0, share, rss[key]
  }
  if (insert_count > 0) {
    print ""
    print "| insert source | key type | rows | stored | ms | rows/sec |"
    print "|---|---|---|---|---|---|"
    for (row = 1; row <= insert_count; row++) {
      key = insert_order[row]
      split(key, parts, "|")
      printf "| %s | %s | %s | %s | %.1f | %s |\n",
        parts[1], parts[2], insert_rows[key], insert_stored[key],
        insert_us[key] / 1000.0, insert_rate[key]
    }
  }
}
