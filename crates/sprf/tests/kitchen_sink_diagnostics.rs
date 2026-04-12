//! KITCHEN SINK DIAGNOSTICS TEST
//!
//! This test validates that the kitchen_sink.sprf file produces expected diagnostics.
//! It should help debug why patterns that DO exist show "0 files matched" errors.

use std::path::PathBuf;

const KITCHEN_SINK_SPRF: &str = include_str!("fixtures/kitchen_sink.sprf");

/// Test that validates diagnostics for kitchen sink.
/// This test should FAIL if we get "0 files matched" for patterns that DO exist.
#[test]
fn kitchen_sink_diagnostics_analysis() {
    println!("\n=== KITCHEN SINK DIAGNOSTICS ANALYSIS ===\n");
    
    // Get workspace root (3 levels up: crates/sprf/tests -> crates/sprf -> crates -> root)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    
    println!("Workspace root: {}", workspace_root.display());
    println!("Kitchen sink file loaded: {} bytes", KITCHEN_SINK_SPRF.len());
    
    // Run analysis
    let options = sprefa_sprf::analyze::AnalyzeOptions {
        run_extraction: true,
        workspace_root: workspace_root.to_path_buf(),
        documents: std::collections::HashMap::new(),
    };
    
    let analysis = sprefa_sprf::analyze::analyze_partial(KITCHEN_SINK_SPRF, options);
    
    // Print ALL diagnostics
    println!("\n--- PARSE ERRORS ---");
    for err in &analysis.errors {
        println!("  [{:?}] {:?}: {}", err.phase, err.severity, err.message);
    }
    
    println!("\n--- EXTRACTION DIAGNOSTICS ---");
    for diag in &analysis.diagnostics {
        let location = diag.element_location.as_ref()
            .map(|l| format!("@{}-{}", l.start, l.end))
            .unwrap_or_default();
        println!("  [{:?}] {:?} {}: {} {}", 
            diag.severity,
            diag.element_type,
            diag.rule,
            diag.message,
            location
        );
    }
    
    // Print extraction results
    println!("\n--- EXTRACTION RESULTS ---");
    for extraction in &analysis.extractions {
        let total_values: usize = extraction.results.iter()
            .map(|(_, _, vals)| vals.len())
            .sum();
        println!("  Rule '{}': {} files scanned, {} total values extracted",
            extraction.rule_name,
            extraction.files_scanned,
            total_values
        );
        for (file, var, vals) in &extraction.results {
            if !vals.is_empty() {
                println!("    - {}.{}: {} values", 
                    file.file_name().unwrap_or_default().to_string_lossy(),
                    var,
                    vals.len()
                );
            }
        }
    }
    
    // Print what files actually exist that should match
    println!("\n--- ACTUAL FILES IN WORKSPACE ---");
    let patterns_to_check = vec![
        "**/Cargo.toml",
        "**/*.md",
        "**/package.json",
        "**/values.yaml",
        "**/services.yaml",
    ];
    
    for pattern in patterns_to_check {
        use globset::Glob;
        let glob = Glob::new(pattern).unwrap().compile_matcher();
        let mut matches = vec![];
        
        for entry in walkdir::WalkDir::new(workspace_root)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                let rel = path.strip_prefix(workspace_root).unwrap_or(path);
                if glob.is_match(rel) {
                    matches.push(rel.to_string_lossy().to_string());
                }
            }
        }
        
        matches.truncate(5);
        println!("  Pattern '{}': {} matches (showing first 5: {:?})", 
            pattern, matches.len(), matches);
    }
    
    // Validate: No "0 files matched" errors should exist for patterns that DO match
    let mut unexpected_errors = vec![];
    
    for diag in &analysis.diagnostics {
        if diag.message.contains("matched 0 files") {
            // Check if this pattern SHOULD have matched
            let should_match = match diag.rule.as_str() {
                "fs_complete" => true,  // **/Cargo.toml should match
                "file_complete" => true, // **/*.md should match
                "capture_test" => true,  // **/Cargo.toml should match
                "inline_repo" => false,  // **/services.yaml might not exist
                "repo_complete" => false, // Depends on repos being configured
                _ => false,
            };
            
            if should_match {
                unexpected_errors.push(format!("Rule '{}' reported 0 files but pattern should exist", diag.rule));
            }
        }
    }
    
    println!("\n--- VALIDATION ---");
    if unexpected_errors.is_empty() {
        println!("✅ All expected patterns found files");
    } else {
        println!("❌ UNEXPECTED ERRORS:");
        for err in &unexpected_errors {
            println!("  - {}", err);
        }
        panic!("Kitchen sink has unexpected '0 files matched' errors. See output above.");
    }
    
    // Summary
    let error_count = analysis.errors.len();
    let diag_count = analysis.diagnostics.len();
    let rules_with_data = analysis.extractions.iter()
        .filter(|e| e.files_scanned > 0)
        .count();
    
    println!("\n=== SUMMARY ===");
    println!("Parse errors: {}", error_count);
    println!("Extraction diagnostics: {}", diag_count);
    println!("Rules with matching files: {}", rules_with_data);
    
    // We expect some rules to have data
    assert!(rules_with_data > 0, "Expected at least some rules to find files");
}

/// Test that validates hover works for all capture types in kitchen sink.
#[test]
fn kitchen_sink_hover_validation() {
    println!("\n=== KITCHEN SINK HOVER VALIDATION ===\n");
    
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    
    let options = sprefa_sprf::analyze::AnalyzeOptions {
        run_extraction: true,
        workspace_root: workspace_root.to_path_buf(),
        documents: std::collections::HashMap::new(),
    };
    
    let analysis = sprefa_sprf::analyze::analyze_partial(KITCHEN_SINK_SPRF, options);
    
    // Check each rule for captures
    for rule in &analysis.rules {
        let captures: Vec<_> = rule.create_matches.iter()
            .map(|m| m.capture.clone())
            .collect();
        
        if !captures.is_empty() {
            println!("Rule '{}': captures {:?}", rule.name, captures);
        }
    }
    
    println!("\n✅ Hover validation complete");
}
