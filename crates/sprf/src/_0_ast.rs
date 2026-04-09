/// Parse tree for .sprf files.
///
/// .sprf text -> Vec<Statement> -> lower -> Vec<Rule>

/// A complete .sprf file: a sequence of statements.
pub type Program = Vec<Statement>;

/// One top-level statement.
#[derive(Debug, Clone)]
pub enum Statement {
    Rule(RuleDecl),
    Check(CheckDecl),
}

/// A rule with a name and body in braces.
///
/// ```sprf
/// rule(deploy_config) {
///     repo($REPO) {
///         rev(main) {
///             fs(**/services.yaml) > json({ ... })
///         }
///     }
/// };
/// ```
///
/// Captures are inferred from `$VAR` usage in the body during lowering.
/// The body is a list of RuleBody items, allowing flat chains and scoped blocks.
#[derive(Debug, Clone)]
pub struct RuleDecl {
    pub name: String,
    pub body: Vec<RuleBody>,
}

/// A check block: `check(name) { SQL };`.
///
/// ```sprf
/// check(openapi_drift) {
///     SELECT m.name, m.version, s.version
///     FROM mono_openapi_data m, sot_openapi_data s
///     WHERE strip_suffixes(m.name, '-service') = s.name
///       AND m.version != s.version
/// };
/// ```
///
/// Semantics: rows returned = violations to insert into invariant_violations.
#[derive(Debug, Clone)]
pub struct CheckDecl {
    pub name: String,
    pub sql: String,
}

/// A cross-rule reference: `rulename(col: $VAR, col: $VAR)`.
///
/// Binds columns from a previously-evaluated rule's output table.
/// Parsed as a dependency edge during lowering.
#[derive(Debug, Clone)]
pub struct CrossRef {
    pub rule_name: String,
    pub bindings: Vec<CrossRefBinding>,
}

/// One column binding in a cross-rule reference.
#[derive(Debug, Clone)]
pub struct CrossRefBinding {
    /// Column name in the referenced rule's output table.
    pub column: String,
    /// Local variable to bind the column value to.
    pub var: String,
}

/// Recursive rule body: steps, nested scopes, or cross-rule references.
///
/// A step is a single matcher like `fs(**/file)`. A block introduces a scope
/// that can contain nested bodies. Variables captured in a block are available
/// to all nested children. A cross-ref binds upstream rule output columns.
#[derive(Debug, Clone)]
pub enum RuleBody {
    /// A single step in the chain (leaf or non-scoped match).
    Step(Slot),
    /// A scoped block: the slot captures variables that flow into children.
    ///
    /// `is_chain` is true when children came from a `>` pipeline (A > B > C);
    /// false when they came from a `{ ... }` brace block (A { B; C }).
    /// Only brace-block children represent alternative branches for monomorphization.
    Block { slot: Slot, children: Vec<RuleBody>, is_chain: bool },
    /// A cross-rule reference, optionally scoping a block of children.
    ///
    /// Example: `helm_image(repo: $REPO, rev: $TAG) { fs(...) }`
    /// Binds columns from the referenced rule's output, optionally scoping
    /// children under each row.
    Ref {
        cross_ref: CrossRef,
        children: Vec<RuleBody>,
    },
}

/// One segment of a selector.
#[derive(Debug, Clone)]
pub enum Slot {
    /// A bare glob string (no tag). Inferred context during lowering.
    Bare(String),
    /// A tagged slot: `tag(body)` or `tag[arg](body)`.
    Tagged {
        tag: Tag,
        arg: Option<String>,
        body: String,
    },
}

/// Known tag names in `tag(body)` notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Json,
    Line,
    Ast,
    Repo,
    Rev,
    Folder,
    File,
    Fs,
}

impl Tag {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Tag::Json),
            "line" => Some(Tag::Line),
            "ast" => Some(Tag::Ast),
            "repo" => Some(Tag::Repo),
            "rev" | "branch" | "tag" => Some(Tag::Rev),
            "folder" => Some(Tag::Folder),
            "file" => Some(Tag::File),
            "fs" => Some(Tag::Fs),
            _ => None,
        }
    }
}

impl RuleBody {
    /// Collect all captures from slots at this level and nested levels.
    /// Used to validate that head captures are actually captured somewhere.
    pub fn all_captures(&self) -> Vec<String> {
        let mut caps = Vec::new();
        self.collect_captures(&mut caps);
        caps
    }

    fn collect_captures(&self, out: &mut Vec<String>) {
        match self {
            RuleBody::Step(slot) => {
                out.extend(slot.captures());
            }
            RuleBody::Block { slot, children, .. } => {
                out.extend(slot.captures());
                for child in children {
                    child.collect_captures(out);
                }
            }
            RuleBody::Ref {
                cross_ref,
                children,
            } => {
                for binding in &cross_ref.bindings {
                    out.push(binding.var.clone());
                }
                for child in children {
                    child.collect_captures(out);
                }
            }
        }
    }

    /// Flatten to a list of (scope_depth, slot) pairs for lowering.
    /// Depth 0 = top level, depth 1 = first block, etc.
    pub fn flatten(&self) -> Vec<(usize, Slot)> {
        let mut result = Vec::new();
        self.flatten_with_depth(0, &mut result);
        result
    }

    fn flatten_with_depth(&self, depth: usize, out: &mut Vec<(usize, Slot)>) {
        match self {
            RuleBody::Step(slot) => {
                out.push((depth, slot.clone()));
            }
            RuleBody::Block { slot, children, .. } => {
                // The block slot is at current depth
                out.push((depth, slot.clone()));
                // Children are at next depth level
                for child in children {
                    child.flatten_with_depth(depth + 1, out);
                }
            }
            RuleBody::Ref { children, .. } => {
                // Cross-ref itself doesn't produce a slot, but children do
                for child in children {
                    child.flatten_with_depth(depth + 1, out);
                }
            }
        }
    }
}

impl Slot {
    /// Extract $SCREAMING capture variables from slot body.
    pub fn captures(&self) -> Vec<String> {
        match self {
            Slot::Bare(glob) => extract_captures_from_str(glob),
            Slot::Tagged { body, .. } => extract_captures_from_str(body),
        }
    }

    /// Get the tag if this is a tagged slot, None for bare.
    pub fn tag(&self) -> Option<Tag> {
        match self {
            Slot::Bare(_) => None,
            Slot::Tagged { tag, .. } => Some(*tag),
        }
    }
}

/// Extract $SCREAMING variables and `(?P<NAME>...)` regex named groups from a string.
fn extract_captures_from_str(s: &str) -> Vec<String> {
    let mut caps = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            // Check for wildcard $_
            if i < bytes.len()
                && bytes[i] == b'_'
                && (i + 1 >= bytes.len() || !bytes[i + 1].is_ascii_alphanumeric())
            {
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > start {
                let name = &s[start..i];
                if name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
                {
                    caps.push(name.to_string());
                }
            }
        } else if bytes[i] == b'('
            && i + 3 < bytes.len()
            && bytes[i + 1] == b'?'
            && bytes[i + 2] == b'P'
            && bytes[i + 3] == b'<'
        {
            // (?P<NAME>...) regex named group
            i += 4; // skip (?P<
            let start = i;
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            if i > start {
                caps.push(s[start..i].to_string());
            }
            if i < bytes.len() {
                i += 1; // skip >
            }
        } else {
            i += 1;
        }
    }
    caps
}
