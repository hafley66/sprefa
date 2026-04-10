/**
 * MULTI-TARGET OUTPUT ARCHITECTURE
 * 
 * One extraction, multiple outputs:
 * - SQL → SQLite for querying
 * - JSON → CI artifacts, other tools
 * - Raw → Internal processing
 */

// ============================================================================
// THE EXTRACTOR (Source of Truth)
// ============================================================================

// This runs once per file, produces structured results
interface Extractor {
  // Walk the file, find matches, return captures
  extract(filePath: string, content: string): ExtractionResult[];
}

interface ExtractionResult {
  ruleName: string;
  filePath: string;
  scopeId: number;
  captures: Map<string, string>;
  // Plus metadata for different outputs
  span?: { start: number; end: number };  // For source maps
}

// ============================================================================
// OUTPUT TARGETS (Pluggable Sinks)
// ============================================================================

// Each target receives extraction results and formats them
interface OutputTarget {
  name: string;
  init(): Promise<void>;
  write(results: ExtractionResult[]): Promise<void>;
  finalize(): Promise<void>;
}

// SQL Target: Write to SQLite
class SqliteTarget implements OutputTarget {
  name = 'sqlite';
  private db: Database;
  
  async init() {
    this.db = await open('sprefa.db');
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS matches (
        id INTEGER PRIMARY KEY,
        rule_name TEXT,
        file_path TEXT,
        scope_id INTEGER
      );
      CREATE TABLE IF NOT EXISTS captures (
        match_id INTEGER,
        var_name TEXT,
        value TEXT
      );
    `);
  }
  
  async write(results: ExtractionResult[]) {
    for (const r of results) {
      const matchId = await this.db.run(
        'INSERT INTO matches (rule_name, file_path, scope_id) VALUES (?, ?, ?)',
        [r.ruleName, r.filePath, r.scopeId]
      );
      
      for (const [varName, value] of r.captures) {
        await this.db.run(
          'INSERT INTO captures (match_id, var_name, value) VALUES (?, ?, ?)',
          [matchId, varName, value]
        );
      }
    }
  }
  
  async finalize() {
    // Create indices for fast queries
    await this.db.exec('CREATE INDEX IF NOT EXISTS idx_captures_match ON captures(match_id)');
    await this.db.close();
  }
}

// JSON Target: CI-friendly output
class JsonTarget implements OutputTarget {
  name = 'json';
  private outputPath: string;
  private results: ExtractionResult[] = [];
  
  constructor(outputPath: string) {
    this.outputPath = outputPath;
  }
  
  async init() {
    // Nothing needed
  }
  
  async write(results: ExtractionResult[]) {
    this.results.push(...results);
  }
  
  async finalize() {
    // Group by rule for cleaner output
    const grouped: Record<string, any[]> = {};
    
    for (const r of this.results) {
      if (!grouped[r.ruleName]) grouped[r.ruleName] = [];
      
      // Convert captures to object
      const captureObj: Record<string, string> = {};
      for (const [k, v] of r.captures) {
        captureObj[k] = v;
      }
      
      grouped[r.ruleName].push({
        file: r.filePath,
        captures: captureObj,
        // Add span if available
        ...(r.span && { location: r.span })
      });
    }
    
    const output = {
      generatedAt: new Date().toISOString(),
      totalMatches: this.results.length,
      rules: grouped
    };
    
    await fs.writeFile(this.outputPath, JSON.stringify(output, null, 2));
  }
}

// NDJSON Target: Streaming for large outputs
class NdjsonTarget implements OutputTarget {
  name = 'ndjson';
  private outputPath: string;
  private stream: WriteStream;
  
  constructor(outputPath: string) {
    this.outputPath = outputPath;
  }
  
  async init() {
    this.stream = createWriteStream(this.outputPath);
  }
  
  async write(results: ExtractionResult[]) {
    for (const r of results) {
      const captureObj: Record<string, string> = {};
      for (const [k, v] of r.captures) {
        captureObj[k] = v;
      }
      
      const line = JSON.stringify({
        rule: r.ruleName,
        file: r.filePath,
        captures: captureObj
      });
      
      this.stream.write(line + '\n');
    }
  }
  
  async finalize() {
    this.stream.end();
    await new Promise((resolve) => this.stream.on('finish', resolve));
  }
}

// Summary Target: Human-readable for CI logs
class SummaryTarget implements OutputTarget {
  name = 'summary';
  private results: ExtractionResult[] = [];
  
  async init() {}
  
  async write(results: ExtractionResult[]) {
    this.results.push(...results);
  }
  
  async finalize() {
    // Group and count
    const byRule: Record<string, number> = {};
    for (const r of this.results) {
      byRule[r.ruleName] = (byRule[r.ruleName] || 0) + 1;
    }
    
    console.log('\n=== SPREFA EXTRACTION SUMMARY ===');
    console.log(`Total matches: ${this.results.length}`);
    console.log('\nBy rule:');
    for (const [rule, count] of Object.entries(byRule).sort((a, b) => b[1] - a[1])) {
      console.log(`  ${rule}: ${count}`);
    }
    console.log('==================================\n');
  }
}

// ============================================================================
// MULTI-TARGET RUNNER
// ============================================================================

class MultiTargetRunner {
  private targets: OutputTarget[] = [];
  
  addTarget(target: OutputTarget) {
    this.targets.push(target);
  }
  
  async run(extractor: Extractor, files: string[]) {
    // Initialize all targets
    for (const t of this.targets) {
      await t.init();
    }
    
    // Process files one at a time (streaming)
    for (const file of files) {
      const content = await fs.readFile(file, 'utf-8');
      const results = extractor.extract(file, content);
      
      // Write to all targets
      for (const t of this.targets) {
        await t.write(results);
      }
    }
    
    // Finalize all targets
    for (const t of this.targets) {
      await t.finalize();
    }
  }
}

// ============================================================================
// USAGE EXAMPLES
// ============================================================================

// Example 1: SQL only (for local querying)
async function runSqlMode() {
  const runner = new MultiTargetRunner();
  runner.addTarget(new SqliteTarget());
  
  // Run extraction
  // await runner.run(extractor, files);
  
  // Now you can:
  // sqlite3 sprefa.db "SELECT * FROM captures WHERE var_name = 'PACKAGE'"
}

// Example 2: JSON for CI
async function runCiMode() {
  const runner = new MultiTargetRunner();
  runner.addTarget(new JsonTarget('ci-results.json'));
  runner.addTarget(new SummaryTarget()); // Also log to console
  
  // Run extraction
  // await runner.run(extractor, files);
  
  // CI artifact: ci-results.json
  // GitHub Actions can upload this as an artifact
  // Other tools can read the JSON
}

// Example 3: Both SQL and JSON
async function runHybridMode() {
  const runner = new MultiTargetRunner();
  runner.addTarget(new SqliteTarget());
  runner.addTarget(new JsonTarget('results.json'));
  runner.addTarget(new NdjsonTarget('results.ndjson'));
  
  // Run once, get all three outputs
  // await runner.run(extractor, files);
}

// Example 4: Large repo, streaming NDJSON
async function runStreamingMode() {
  const runner = new MultiTargetRunner();
  runner.addTarget(new NdjsonTarget('massive-output.ndjson'));
  // No SQLite - don't want to hold everything in memory
  
  // Process 100k files, stream results
  // Memory stays flat
}


// ============================================================================
// CLI INTEGRATION
// ============================================================================

/*
$ sprf scan --output sqlite:results.db
$ sprf scan --output json:ci-results.json
$ sprf scan --output ndjson:stream.jsonl
$ sprf scan --output summary

# Multiple outputs
$ sprf scan --output sqlite:results.db --output json:results.json --output summary

# Or use .sprf config:
output:
  - sqlite: sprefa.db
  - json: results.json
  - summary: true
*/


// ============================================================================
// RUST EQUIVALENT
// ============================================================================

/*
// File: crates/sprf/src/output.rs

pub trait OutputTarget: Send + Sync {
    fn name(&self) -> &str;
    fn init(&mut self) -> Result<()>;
    fn write(&mut self, results: &[ExtractionResult]) -> Result<()>;
    fn finalize(&mut self) -> Result<()>;
}

pub struct SqliteTarget {
    db: Option<Connection>,
}

impl OutputTarget for SqliteTarget {
    fn name(&self) -> &str { "sqlite" }
    
    fn init(&mut self) -> Result<()> {
        self.db = Some(Connection::open("sprefa.db")?);
        // Create tables...
        Ok(())
    }
    
    fn write(&mut self, results: &[ExtractionResult]) -> Result<()> {
        let db = self.db.as_ref().unwrap();
        for r in results {
            let match_id = db.execute(
                "INSERT INTO matches (rule_name, file_path, scope_id) VALUES (?1, ?2, ?3)",
                [&r.rule_name, &r.file_path, &r.scope_id.to_string()],
            )?;
            // Insert captures...
        }
        Ok(())
    }
    
    fn finalize(&mut self) -> Result<()> {
        // Create indices, close
        Ok(())
    }
}

pub struct JsonTarget {
    path: PathBuf,
    results: Vec<ExtractionResult>,
}

impl OutputTarget for JsonTarget {
    fn name(&self) -> &str { "json" }
    
    fn init(&mut self) -> Result<()> { Ok(()) }
    
    fn write(&mut self, results: &[ExtractionResult]) -> Result<()> {
        self.results.extend_from_slice(results);
        Ok(())
    }
    
    fn finalize(&mut self) -> Result<()> {
        let output = serde_json::to_string_pretty(&self.results)?;
        fs::write(&self.path, output)?;
        Ok(())
    }
}

// Runner
pub struct MultiTargetRunner {
    targets: Vec<Box<dyn OutputTarget>>,
}

impl MultiTargetRunner {
    pub fn add_target(&mut self, target: Box<dyn OutputTarget>) {
        self.targets.push(target);
    }
    
    pub async fn run(
        &mut self,
        extractor: &dyn Extractor,
        files: &[PathBuf],
    ) -> Result<()> {
        // Init all
        for t in &mut self.targets {
            t.init()?;
        }
        
        // Process files
        for file in files {
            let content = fs::read_to_string(file)?;
            let results = extractor.extract(file, &content);
            
            // Write to all targets
            for t in &mut self.targets {
                t.write(&results)?;
            }
        }
        
        // Finalize all
        for t in &mut self.targets {
            t.finalize()?;
        }
        
        Ok(())
    }
}

// Usage:
let mut runner = MultiTargetRunner::new();
runner.add_target(Box::new(SqliteTarget::new()));
runner.add_target(Box::new(JsonTarget::new("results.json")));
runner.run(&extractor, &files).await?;
*/


// ============================================================================
// KEY INSIGHT
// ============================================================================

/*
The extraction logic stays the SAME.
Only the OUTPUT changes.

This is the "write once, output everywhere" pattern:
- Extract once (expensive: AST parsing, walking)
- Write many (cheap: just formatting)

For CI: JSON artifact that other tools consume
For local: SQLite for ad-hoc queries
For large repos: NDJSON streaming
For debugging: Summary to console

All from the same extraction pass.
*/

console.log("Multi-target architecture ready!");
console.log("Run: sprf scan --output sqlite:db --output json:ci.json --output summary");
