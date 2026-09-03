//! Deterministic generated-text regions projected into Soopy mutations.
//!
//! Marker recognition and text rendering are pure. Applying a proposal stays
//! behind Soopy's expected-content stage and commit boundaries.

use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedRegion {
    pub id: String,
    pub start: u64,
    pub end: u64,
    pub current: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedRegionProposal {
    pub region: OwnedRegion,
    pub expected: soopy::ContentId,
    pub replacement: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnedRegionError {
    InvalidId(String),
    NonUtf8,
    MissingBegin(String),
    MissingEnd(String),
    DuplicateBegin(String),
    DuplicateEnd(String),
    EndBeforeBegin(String),
}

impl fmt::Display for OwnedRegionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for OwnedRegionError {}

pub fn owned_region_markers(id: &str) -> Result<(String, String), OwnedRegionError> {
    if id.is_empty() || id.chars().any(|character| character.is_whitespace()) {
        return Err(OwnedRegionError::InvalidId(id.to_string()));
    }
    Ok((
        format!("; sprefa:auto-begin {id}"),
        format!("; sprefa:auto-end {id}"),
    ))
}

pub fn find_owned_region(content: &[u8], id: &str) -> Result<OwnedRegion, OwnedRegionError> {
    let text = std::str::from_utf8(content).map_err(|_| OwnedRegionError::NonUtf8)?;
    let (begin, end) = owned_region_markers(id)?;
    let begin_offset = unique_marker(text, &begin).map_err(|count| match count {
        0 => OwnedRegionError::MissingBegin(id.to_string()),
        _ => OwnedRegionError::DuplicateBegin(id.to_string()),
    })?;
    let end_offset = unique_marker(text, &end).map_err(|count| match count {
        0 => OwnedRegionError::MissingEnd(id.to_string()),
        _ => OwnedRegionError::DuplicateEnd(id.to_string()),
    })?;
    let start = line_end(text, begin_offset + begin.len());
    if end_offset < start {
        return Err(OwnedRegionError::EndBeforeBegin(id.to_string()));
    }
    Ok(OwnedRegion {
        id: id.to_string(),
        start: start as u64,
        end: end_offset as u64,
        current: text[start..end_offset].to_string(),
    })
}

fn unique_marker(text: &str, marker: &str) -> Result<usize, usize> {
    let found = text
        .match_indices(marker)
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [offset] => Ok(*offset),
        _ => Err(found.len()),
    }
}

fn line_end(text: &str, offset: usize) -> usize {
    match text.as_bytes()[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
    {
        Some(relative) => offset + relative + 1,
        None => offset,
    }
}

pub fn propose_owned_region(
    content: &[u8],
    id: &str,
    generated: &str,
) -> Result<OwnedRegionProposal, OwnedRegionError> {
    let region = find_owned_region(content, id)?;
    Ok(OwnedRegionProposal {
        region,
        expected: soopy::ContentId::blake3(content),
        replacement: normalized_generated(generated),
    })
}

fn normalized_generated(generated: &str) -> String {
    if generated.is_empty() || generated.ends_with('\n') {
        generated.to_string()
    } else {
        format!("{generated}\n")
    }
}

impl OwnedRegionProposal {
    pub fn changed(&self) -> bool {
        self.region.current != self.replacement
    }

    pub fn stage_request(
        &self,
        root: soopy::SourceRootId,
        source: soopy::ActionSource,
        producer: soopy::ActionProducer,
    ) -> soopy::StageRequest {
        let edit = soopy::TextEdit {
            range: soopy::ActionSpan {
                source: source.clone(),
                start: self.region.start,
                end: self.region.end,
            },
            replacement: self.replacement.as_bytes().to_vec(),
            producer: producer.with_rule(self.region.id.clone()),
        };
        soopy::StageRequest::new(
            root,
            vec![soopy::SourceAction::Replace {
                source,
                expected: self.expected.clone(),
                edits: vec![edit],
            }],
        )
    }
}
