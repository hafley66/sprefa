# sprf-lsp: LSP Server for .sprf Files

## Architecture

The LSP server is built on a **unified analysis pipeline** that combines parse, lower, and extract phases with shared error recovery. This keeps the LSP layer thin - it's essentially just protocol conversion.

```
LSP Layer (main.rs)
  - Protocol handling (tower-lsp)
  - Request routing
  - Thin wrapper around unified analysis

Unified Analysis (sprefa_sprf::analyze)
  - parse_phase():   source → stmts + errors
  - lower_phase():   stmts → rules + symbol_table
  - extract_phase(): rules → extractions + diagnostics
  - Error recovery at each phase

LSP State (state.rs)
  - AnalysisCache: uri → PartialAnalysis
  - DocumentStore: uri → unsaved content
  - Version tracking for incremental sync
```

## Unified Analysis Pipeline

The `analyze_partial()` function is the single entry point for all analysis.

### Error Recovery

Each phase has error recovery:
1. **Parse Phase**: Recovers at statement boundaries
2. **Lower Phase**: Per-statement error handling
3. **Extract Phase**: Per-rule extraction

## LSP Capabilities

| Feature | Status |
|---------|--------|
| Parse error diagnostics | Live |
| Extraction diagnostics | Live |
| Tag completions | Yes |
| Capture completions | Yes |
| Cross-ref completions | Yes |
| Hover (captures) | Yes |
| Hover (file patterns) | Yes |
| Goto Definition | Yes |
| Document sync | Full |

## State Management

### Analysis Cache

- Cache keyed by URI with version checking
- LRU eviction (default max 100 entries)
- Invalidated on document close

### Document Store

Stores unsaved content for extraction from modified files.

## Testing

Kitchen sink tests in `main.rs` under `#[cfg(test)]` verify partial parsing recovery.

## Migration Notes

The codebase is transitioning from legacy separate operations to unified analysis.
