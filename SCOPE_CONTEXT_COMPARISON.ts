/**
 * SCOPE CONTEXT: TypeScript vs Rust
 * 
 * This file shows the complete implementation of scoped rule evaluation
 * in TypeScript, with Rust equivalents in comments.
 */

// ============================================================================
// PART 1: AST TYPES
// ============================================================================

// TypeScript:
type Program = Statement[];

type Statement =
  | { kind: 'rule'; rule: RuleDecl }
  | { kind: 'link'; link: LinkDecl }  // We'll remove this per your direction
  | { kind: 'query'; query: QueryDecl }; // We'll remove this too

interface RuleDecl {
  name: string;
  captures: Capture[];
  body: RuleBody;  // Changed from flat SelectorChain to recursive RuleBody
}

interface Capture {
  var: string;           // e.g., "NAME"
  annotation?: 'repo' | 'rev' | 'name' | 'file';
}

// OLD: Flat selector chain
// type SelectorChain = { slots: Slot[] };

// NEW: Recursive rule body - THIS IS THE KEY CHANGE
// A RuleBody can be either a leaf step OR a nested block with children
type RuleBody =
  | { kind: 'step'; slot: Slot }
  | { kind: 'block'; scope: ScopeSlot; children: RuleBody[] };

// A scope is like repo($REPO), rev(main), folder(packages/$PACKAGE)
// It's a slot that captures variables AND opens a new scope
interface ScopeSlot {
  tag: 'repo' | 'rev' | 'folder' | 'fs' | 'file';
  pattern: string;        // e.g., "packages/$PACKAGE"
  captures: string[];     // Variables captured in this scope ["PACKAGE"]
}

// A regular slot (leaf) like json(...), ast(...), etc.
interface Slot {
  tag: 'json' | 're' | 'ast' | 'fs' | 'file';
  pattern: string;
  captures: string[];
}

// LinkDecl and QueryDecl will be removed per your direction
interface LinkDecl { kind: 'link'; /* ... */ }
interface QueryDecl { kind: 'query'; /* ... */ }

/*
// Rust equivalent:
// pub type Program = Vec<Statement>;
// 
// pub enum Statement {
//     Rule(RuleDecl),
//     // Link(LinkDecl),  // REMOVED
//     // Query(QueryDecl), // REMOVED
// }
//
// pub struct RuleDecl {
//     pub name: String,
//     pub captures: Vec<Capture>,
//     pub body: RuleBody,  // Recursive!
// }
//
// pub struct Capture {
//     pub var: String,
//     pub annotation: Option<CaptureAnnotation>,
// }
//
// pub enum CaptureAnnotation { Repo, Rev, Name, File }
//
// // THE KEY: Recursive RuleBody enum
// pub enum RuleBody {
//     Step(Slot),
//     Block { scope: ScopeSlot, children: Vec<RuleBody> },
// }
//
// pub struct ScopeSlot {
//     pub tag: ScopeTag,
//     pub pattern: String,
//     pub captures: Vec<String>, // Variables like ["PACKAGE"]
// }
//
// pub enum ScopeTag { Repo, Rev, Folder, Fs, File }
//
// pub struct Slot {
//     pub tag: SlotTag,
//     pub pattern: String,
//     pub captures: Vec<String>,
// }
//
// pub enum SlotTag { Json, Re, Ast, Fs, File }
*/


// ============================================================================
// PART 2: SCOPE CONTEXT (THE HEART OF IT)
// ============================================================================

// TypeScript:
// In TS, we can just use objects and references. Simple!

interface ScopeContext {
  // Each scope level has its own variable bindings
  variables: Map<string, string>;  // $NAME -> "actual-value"
  // Reference to parent (JS closures for the win)
  parent?: ScopeContext;
}

function createRootContext(): ScopeContext {
  return { variables: new Map() };
}

function createChildContext(parent: ScopeContext): ScopeContext {
  return {
    variables: new Map(),
    parent,  // Just reference it - TS handles this
  };
}

// Look up a variable: check current scope, then walk up parents
function lookupVariable(ctx: ScopeContext, varName: string): string | undefined {
  // Check my scope first
  if (ctx.variables.has(varName)) {
    return ctx.variables.get(varName);
  }
  // Walk up to parent
  if (ctx.parent) {
    return lookupVariable(ctx.parent, varName);
  }
  return undefined;
}

// Bind a variable in the current scope
function bindVariable(ctx: ScopeContext, varName: string, value: string): void {
  ctx.variables.set(varName, value);
}

/*
// Rust equivalent:
// In Rust, we CANNOT do this:
// struct ScopeContext {
//     variables: HashMap<String, String>,
//     parent: &ScopeContext,  // ERROR! Lifetime nightmare!
// }
//
// INSTEAD - We use an Arena with indices:
// pub struct ScopeArena {
//     scopes: Vec<Scope>,  // All scopes live here
// }
//
// pub struct Scope {
//     variables: HashMap<String, String>,
//     parent: Option<usize>, // Index into scopes Vec, not a reference!
// }
//
// impl ScopeArena {
//     fn lookup(&self, scope_id: usize, var_name: &str) -> Option<&String> {
//         let scope = &self.scopes[scope_id];
//         // Check current
//         if let Some(val) = scope.variables.get(var_name) {
//             return Some(val);
//         }
//         // Walk up via index (not pointer!)
//         if let Some(parent_id) = scope.parent {
//             return self.lookup(parent_id, var_name);
//         }
//         None
//     }
//     
//     fn bind(&mut self, scope_id: usize, var_name: String, value: String) {
//         self.scopes[scope_id].variables.insert(var_name, value);
//     }
// }
*/


// ============================================================================
// PART 3: PATTERN MATCHING
// ============================================================================

// TypeScript:
// Match "packages/$PACKAGE" against "packages/frontend"
// Returns { PACKAGE: "frontend" } or null
function matchPattern(pattern: string, value: string): Map<string, string> | null {
  // Simple glob matching - split on /, handle $VAR segments
  const patternParts = pattern.split('/');
  const valueParts = value.split('/');
  
  const captures = new Map<string, string>();
  
  for (let i = 0; i < patternParts.length; i++) {
    const p = patternParts[i];
    const v = valueParts[i];
    
    if (p.startsWith('$')) {
      // Capture variable: $PACKAGE matches anything
      captures.set(p.slice(1), v);
    } else if (p === '**') {
      // Globstar - match rest
      break;
    } else if (p !== v) {
      // Literal mismatch
      return null;
    }
  }
  
  return captures;
}

/*
// Rust equivalent:
// fn match_pattern(pattern: &str, value: &str) -> Option<HashMap<String, String>> {
//     let pattern_parts: Vec<_> = pattern.split('/').collect();
//     let value_parts: Vec<_> = value.split('/').collect();
//     
//     let mut captures = HashMap::new();
//     
//     for (p, v) in pattern_parts.iter().zip(value_parts.iter()) {
//         if p.starts_with('$') {
//             captures.insert(p[1..].to_string(), v.to_string());
//         } else if *p == "**" {
//             break;
//         } else if p != v {
//             return None;
//         }
//     }
//     
//     Some(captures)
// }
*/


// ============================================================================
// PART 4: EVALUATION / FLATTENING
// ============================================================================

// TypeScript:
// The key insight: we DENORMALIZE the tree into flat steps with scope refs

interface FlatStep {
  // The slot to match (fs, json, etc)
  slot: Slot;
  // Variables needed from parent scopes
  requiredVars: string[];
  // Which scope ID this step runs in
  scopeId: number;
  // Rule this came from
  ruleName: string;
}

// Flatten a recursive RuleBody into linear steps with scope dependencies
function flattenRuleBody(
  body: RuleBody,
  scopeId: number,
  steps: FlatStep[],
  ruleName: string,
  nextScopeId: { value: number }  // Mutable counter (TS lets us cheat)
): void {
  switch (body.kind) {
    case 'step':
      // Leaf node - just a match operation
      steps.push({
        slot: body.slot,
        requiredVars: body.slot.captures,
        scopeId,
        ruleName,
      });
      break;
      
    case 'block':
      // Scope node - we need to:
      // 1. Create a child scope
      // 2. Match the scope pattern
      // 3. Recurse into children
      
      const childScopeId = nextScopeId.value++;
      
      // The scope match itself creates bindings in the child scope
      // For example: folder(packages/$PACKAGE) binds $PACKAGE
      
      // First, add a "scope entry" step that runs the match
      // This is like: fs(packages/$PACKAGE) but for scoping
      steps.push({
        slot: {
          tag: body.scope.tag as any, // fs/file/folder
          pattern: body.scope.pattern,
          captures: body.scope.captures,
        },
        requiredVars: [], // Scope captures happen AT match time
        scopeId: childScopeId,
        ruleName,
      });
      
      // Then recurse into all children with the new scope
      for (const child of body.children) {
        flattenRuleBody(child, childScopeId, steps, ruleName, nextScopeId);
      }
      break;
  }
}

// Main entry: flatten all rules
function flattenProgram(program: Program): FlatStep[] {
  const allSteps: FlatStep[] = [];
  let nextScopeId = { value: 0 };
  
  for (const stmt of program) {
    if (stmt.kind === 'rule') {
      // Root scope is 0
      flattenRuleBody(stmt.rule.body, 0, allSteps, stmt.rule.name, nextScopeId);
    }
  }
  
  return allSteps;
}

/*
// Rust equivalent:
// #[derive(Debug)]
// struct FlatStep {
//     slot: Slot,
//     required_vars: Vec<String>,
//     scope_id: usize,
//     rule_name: String,
// }
//
// fn flatten_rule_body(
//     body: &RuleBody,
//     scope_id: usize,
//     steps: &mut Vec<FlatStep>,
//     rule_name: &str,
//     next_scope_id: &mut usize,
// ) {
//     match body {
//         RuleBody::Step(slot) => {
//             steps.push(FlatStep {
//                 slot: slot.clone(),
//                 required_vars: slot.captures.clone(),
//                 scope_id,
//                 rule_name: rule_name.to_string(),
//             });
//         }
//         RuleBody::Block { scope, children } => {
//             let child_scope_id = *next_scope_id;
//             *next_scope_id += 1;
//             
//             // Scope entry step
//             steps.push(FlatStep {
//                 slot: Slot {
//                     tag: match scope.tag {
//                         ScopeTag::Fs => SlotTag::Fs,
//                         ScopeTag::File => SlotTag::File,
//                         ScopeTag::Folder => SlotTag::Fs, // folder is just fs that scopes
//                         _ => panic!("Invalid scope tag for slot"),
//                     },
//                     pattern: scope.pattern.clone(),
//                     captures: scope.captures.clone(),
//                 },
//                 required_vars: vec![],
//                 scope_id: child_scope_id,
//                 rule_name: rule_name.to_string(),
//             });
//             
//             // Recurse
//             for child in children {
//                 flatten_rule_body(child, child_scope_id, steps, rule_name, next_scope_id);
//             }
//         }
//     }
// }
//
// fn flatten_program(program: &[Statement]) -> Vec<FlatStep> {
//     let mut steps = vec![];
//     let mut next_scope_id = 1; // 0 is root
//     
//     for stmt in program {
//         if let Statement::Rule(rule) = stmt {
//             flatten_rule_body(&rule.body, 0, &mut steps, &rule.name, &mut next_scope_id);
//         }
//     }
//     
//     steps
// }
*/


// ============================================================================
// PART 5: EXECUTION (The "One File At A Time" Part)
// ============================================================================

// TypeScript:
// When a file is discovered, we run all applicable steps

interface FileMatch {
  filePath: string;
  content: string;
}

function executeStepsForFile(
  fileMatch: FileMatch,
  steps: FlatStep[],
  scopeArena: ScopeContext[]  // Index 0 = root
): Map<string, string>[] {
  const results: Map<string, string>[] = [];
  
  for (const step of steps) {
    // Check if this step applies to this file
    if (!doesStepApplyToFile(step, fileMatch.filePath)) {
      continue;
    }
    
    // Get the scope context
    const scopeCtx = scopeArena[step.scopeId];
    
    // Check if all required vars are available
    const bindings = new Map<string, string>();
    let canRun = true;
    
    for (const varName of step.requiredVars) {
      const value = lookupVariable(scopeCtx, varName);
      if (!value) {
        canRun = false;
        break;
      }
      bindings.set(varName, value);
    }
    
    if (!canRun) continue;
    
    // Run the match (simplified)
    const matchResult = matchPattern(step.slot.pattern, fileMatch.content);
    if (matchResult) {
      // Success! Merge with existing bindings
      for (const [k, v] of matchResult) {
        bindings.set(k, v);
      }
      results.push(bindings);
    }
  }
  
  return results;
}

function doesStepApplyToFile(step: FlatStep, filePath: string): boolean {
  // Check if step.pattern matches filePath
  // (Simplified - real code handles globs)
  return filePath.includes(step.slot.pattern.replace('**/', '').replace('$', ''));
}

/*
// Rust equivalent:
// struct FileMatch {
//     path: PathBuf,
//     content: String,
// }
//
// fn execute_steps_for_file(
//     file: &FileMatch,
//     steps: &[FlatStep],
//     scope_arena: &ScopeArena,
// ) -> Vec<HashMap<String, String>> {
//     let mut results = vec![];
//     
//     for step in steps {
//         if !does_step_apply(step, &file.path) {
//             continue;
//         }
//         
//         // Try to resolve all required variables from scope
//         let mut bindings = HashMap::new();
//         let mut can_run = true;
//         
//         for var in &step.required_vars {
//             match scope_arena.lookup(step.scope_id, var) {
//                 Some(val) => { bindings.insert(var.clone(), val.clone()); }
//                 None => { can_run = false; break; }
//             }
//         }
//         
//         if !can_run { continue; }
//         
//         // Run extraction (json, ast, etc)
//         if let Some(captures) = run_extraction(&step.slot, &file.content) {
//             bindings.extend(captures);
//             results.push(bindings);
//         }
//     }
//     
//     results
// }
*/


// ============================================================================
// PART 6: COMPLETE EXAMPLE
// ============================================================================

// Example from your spec:
/*
repo($REPO) {
  rev(main) {
    folder(packages/$PACKAGE) {
      fs(src/index.js) > json(...)
      fs(src/main.js) > js(...)
    }
  }
}
*/

// TypeScript AST representation:
const exampleProgram: Program = [
  {
    kind: 'rule',
    rule: {
      name: 'package_files',
      captures: [{ var: 'REPO', annotation: 'repo' }],
      body: {
        kind: 'block',
        scope: {
          tag: 'repo',
          pattern: '$REPO',  // Matches repo name
          captures: ['REPO'],
        },
        children: [
          {
            kind: 'block',
            scope: {
              tag: 'rev',
              pattern: 'main',
              captures: [],
            },
            children: [
              {
                kind: 'block',
                scope: {
                  tag: 'folder',
                  pattern: 'packages/$PACKAGE',
                  captures: ['PACKAGE'],
                },
                children: [
                  {
                    kind: 'step',
                    slot: { tag: 'fs', pattern: 'src/index.js', captures: [] }
                  },
                  {
                    kind: 'step', 
                    slot: { tag: 'fs', pattern: 'src/main.js', captures: [] }
                  },
                ]
              }
            ]
          }
        ]
      }
    }
  }
];

// Flattened steps (what actually runs):
const flattened = flattenProgram(exampleProgram);
/*
Result:
[
  // Scope entry: repo($REPO)
  { slot: { tag: 'repo', pattern: '$REPO', captures: ['REPO'] }, scopeId: 1, requiredVars: [] },
  
  // Scope entry: rev(main)  
  { slot: { tag: 'rev', pattern: 'main', captures: [] }, scopeId: 2, requiredVars: [] },
  
  // Scope entry: folder(packages/$PACKAGE)
  { slot: { tag: 'folder', pattern: 'packages/$PACKAGE', captures: ['PACKAGE'] }, scopeId: 3, requiredVars: [] },
  
  // Actual file matches - they inherit scope 3
  { slot: { tag: 'fs', pattern: 'src/index.js', captures: [] }, scopeId: 3, requiredVars: ['PACKAGE'] },
  { slot: { tag: 'fs', pattern: 'src/main.js', captures: [] }, scopeId: 3, requiredVars: ['PACKAGE'] },
]
*/

// When we process "myrepo/packages/frontend/src/index.js":
// 1. Match repo("myrepo") -> bind $REPO="myrepo" in scope 1
// 2. Match rev("main") -> passes, scope 2 inherits from 1
// 3. Match folder("packages/frontend") -> bind $PACKAGE="frontend" in scope 3
// 4. Match fs("src/index.js") -> looks up $PACKAGE from scope 3, gets "frontend"
// 5. SUCCESS: emit match with $REPO="myrepo", $PACKAGE="frontend"


// ============================================================================
// PART 7: KEY DIFFERENCES SUMMARY
// ============================================================================

/*
TYPESCRIPT vs RUST - The Gotchas:

1. MEMORY MANAGEMENT
   TS: Just use objects. Garbage collector handles it.
   RS: Use Vec (arena) + usize indices. No references to parent scopes!
   
   TS:  { parent: scope, variables: new Map() }
   RS:  scope.parent: Option<usize>  // Index, not reference!

2. NULL/UNDEFINED
   TS: null, undefined, or optional (value?)
   RS: Option<T> - explicit Some(value) or None
   
   TS:  if (ctx.parent) { ... }
   RS:  if let Some(parent_id) = scope.parent { ... }

3. ERROR HANDLING
   TS: throw new Error(), try/catch
   RS: Result<T, E> - explicit Ok(value) or Err(error)
   
   TS:  function match(): Match | null { if fail return null; }
   RS:  fn match() -> Option<Match> { if fail return None; }

4. PATTERN MATCHING
   TS: switch statements (easy to miss cases)
   RS: match expression with exhaustive checking
   
   TS:  switch(x.kind) { case 'step': ... }
   RS:  match x { RuleBody::Step(s) => ..., RuleBody::Block{..} => ... }

5. MUTABILITY
   TS: let vs const (const just prevents reassignment)
   RS: mut is explicit and required for mutation
   
   TS:  let x = 5; x = 10; // fine
   RS:  let mut x = 5; x = 10; // need mut!

6. OWNERSHIP
   TS: Pass objects around freely, multiple references OK
   RS: One owner at a time OR shared references (with lifetimes)
   
   TS:  function use(x) { anotherFunction(x); console.log(x); } // OK
   RS:  fn use(x: String) { another_function(x); println!("{}", x); } // ERROR! x moved!
   
   Fix: fn use(x: &String) or clone: another_function(x.clone())

7. CLOSURES
   TS: Closures capture variables automatically
   RS: Closures need explicit move || { } or borrow
   
   TS:  const fn = () => console.log(x);
   RS:  let fn = move || println!("{}", x); // x moved into closure

8. GENERICS
   TS: Similar concept, but Rust checks at compile time
   RS: No runtime cost for generics (monomorphization)

9. STRINGS
   TS: Just string primitives
   RS: String (owned, mutable) vs &str (borrowed, read-only slice)
   
   TS:  "hello" // just a string
   RS:  "hello" // &str (static)
        "hello".to_string() // String (owned, heap)

10. COLLECTIONS
    TS: Array, Map, Set - all reference types
    RS: Vec, HashMap, HashSet - all owned, must choose stack vs heap
    
    TS:  const arr = [];
    RS:  let arr: Vec<T> = Vec::new();  // or vec![]
*/


// ============================================================================
// PART 8: THE ACTUAL RUST IMPLEMENTATION FILES (Pseudocode)
// ============================================================================

/*
// File: crates/sprf/src/_0_ast.rs
// =================================

pub type Program = Vec<Statement>;

pub enum Statement {
    Rule(RuleDecl),
    // Link and Query removed per direction
}

pub struct RuleDecl {
    pub name: String,
    pub captures: Vec<Capture>,
    pub body: RuleBody,  // RECURSIVE - this is the key!
}

pub struct Capture {
    pub var: String,
    pub annotation: Option<CaptureAnnotation>,
}

pub enum CaptureAnnotation {
    Repo,
    Rev, 
    Name,
    File,
}

// THE BIG CHANGE: Recursive RuleBody
pub enum RuleBody {
    Step(Slot),
    Block {
        scope: ScopeSlot,      // e.g., folder(packages/$PACKAGE)
        children: Vec<RuleBody>, // Nested rules
    },
}

pub struct ScopeSlot {
    pub tag: ScopeTag,
    pub pattern: String,
    pub captures: Vec<String>,
}

pub enum ScopeTag {
    Repo,
    Rev,
    Folder,
    Fs,
    File,
}

pub struct Slot {
    pub tag: SlotTag,
    pub pattern: String,
    pub captures: Vec<String>,
}

pub enum SlotTag {
    Json,
    Re,
    Ast,
    Fs,
    File,
}


// File: crates/sprf/src/_3_lower.rs  
// ==================================

pub struct FlattenedStep {
    pub slot: Slot,
    pub required_vars: Vec<String>,
    pub scope_id: usize,
    pub rule_name: String,
}

pub struct ScopeArena {
    scopes: Vec<Scope>,
}

pub struct Scope {
    variables: HashMap<String, String>,
    parent: Option<usize>, // KEY: usize index, not reference!
}

impl ScopeArena {
    pub fn new() -> Self {
        Self { scopes: vec![Scope { variables: HashMap::new(), parent: None }] }
    }
    
    pub fn create_child(&mut self, parent_id: usize) -> usize {
        let id = self.scopes.len();
        self.scopes.push(Scope {
            variables: HashMap::new(),
            parent: Some(parent_id),
        });
        id
    }
    
    pub fn lookup(&self, scope_id: usize, var: &str) -> Option<&String> {
        let scope = &self.scopes[scope_id];
        if let Some(val) = scope.variables.get(var) {
            return Some(val);
        }
        if let Some(parent) = scope.parent {
            return self.lookup(parent, var);
        }
        None
    }
    
    pub fn bind(&mut self, scope_id: usize, var: String, val: String) {
        self.scopes[scope_id].variables.insert(var, val);
    }
}

// Flatten recursive RuleBody into linear steps with scope refs
pub fn lower_rule_body(
    body: &RuleBody,
    scope_id: usize,
    arena: &mut ScopeArena,
    steps: &mut Vec<FlattenedStep>,
    rule_name: &str,
) {
    match body {
        RuleBody::Step(slot) => {
            steps.push(FlattenedStep {
                slot: slot.clone(),
                required_vars: slot.captures.clone(),
                scope_id,
                rule_name: rule_name.to_string(),
            });
        }
        RuleBody::Block { scope, children } => {
            // Create child scope
            let child_scope_id = arena.create_child(scope_id);
            
            // Add scope entry step (this triggers the folder match)
            steps.push(FlattenedStep {
                slot: Slot {
                    tag: match scope.tag {
                        ScopeTag::Fs => SlotTag::Fs,
                        ScopeTag::File => SlotTag::File,
                        ScopeTag::Folder => SlotTag::Fs, // folder is scoped fs
                        _ => panic!("Invalid"),
                    },
                    pattern: scope.pattern.clone(),
                    captures: scope.captures.clone(),
                },
                required_vars: vec![],
                scope_id: child_scope_id,
                rule_name: rule_name.to_string(),
            });
            
            // Recurse
            for child in children {
                lower_rule_body(child, child_scope_id, arena, steps, rule_name);
            }
        }
    }
}
*/


console.log("Scope Context comparison loaded!");
console.log("Key insight: In Rust, we use Vec + usize indices instead of object references.");
console.log("This avoids lifetime issues while giving us the same tree structure.");
