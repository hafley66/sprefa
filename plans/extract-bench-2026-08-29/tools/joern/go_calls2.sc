val rows = cpg.call.l.flatMap(c => c.callee.filterNot(_.isExternal).map(m => List(c.method.filename, c.method.name, c.method.fullName, m.filename, m.name, m.fullName).mkString("\t"))).distinct
java.nio.file.Files.write(java.nio.file.Paths.get("/tmp/joern_go3_calls.tsv"), rows.mkString("\n").getBytes)
println("ROWS=" + rows.size + " METHODS=" + cpg.method.size + " FILES=" + cpg.file.size)
