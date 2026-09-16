//! Minimal span-preserving JSON array edits for setup-owned nodes.
use serde_json::Value;

pub(super) fn append_array_value(text: &str, pointer: &str, value: &Value) -> Option<String> {
    let keys: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let mut output = text.to_string();
    loop {
        if let Some((start, end)) = pointer_span(&output, &keys) {
            if !matches!(output.as_bytes().get(start), Some(b'[')) {
                return None;
            }
            let close = end.checked_sub(1)?;
            let occupied = !output[start + 1..close].trim().is_empty();
            let insertion = format!(
                "{}{}",
                if occupied { "," } else { "" },
                serde_json::to_string(value).ok()?
            );
            output.insert_str(close, &insertion);
            return Some(output);
        }
        let mut inserted = false;
        for depth in 0..keys.len() {
            if pointer_span(&output, &keys[..=depth]).is_none() {
                let parent = if depth == 0 {
                    root_span(&output)?
                } else {
                    pointer_span(&output, &keys[..depth])?
                };
                if !matches!(output.as_bytes().get(parent.0), Some(b'{')) {
                    return None;
                }
                let child = if depth + 1 == keys.len() { "[]" } else { "{}" };
                output = insert_object_member(&output, parent, keys[depth], child)?;
                inserted = true;
                break;
            }
        }
        if !inserted {
            return None;
        }
    }
}

pub(super) fn has_pointer(text: &str, pointer: &str) -> bool {
    let keys: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    pointer_span(text, &keys).is_some()
}

pub(super) fn remove_empty_member(
    text: &str,
    parent_pointer: &str,
    key: &str,
    empty: &str,
) -> Option<String> {
    let parent = if parent_pointer.is_empty() {
        root_span(text)?
    } else {
        let keys: Vec<&str> = parent_pointer.trim_start_matches('/').split('/').collect();
        pointer_span(text, &keys)?
    };
    let members = object_members(text, parent)?;
    let (index, member) = members
        .iter()
        .enumerate()
        .find(|(_, member)| member.key == key)?;
    if text[member.value_start..member.value_end].trim() != empty {
        return None;
    }
    let (delete_start, delete_end) = if index + 1 < members.len() {
        let comma = find_byte_forward(
            text,
            member.value_end,
            members[index + 1].member_start,
            b',',
        )?;
        (member.member_start, comma + 1)
    } else if index > 0 {
        let comma = find_byte_backward(
            text,
            members[index - 1].value_end,
            member.member_start,
            b',',
        )?;
        (comma, member.value_end)
    } else {
        (member.member_start, member.value_end)
    };
    let mut output = text.to_string();
    output.replace_range(delete_start..delete_end, "");
    Some(output)
}

pub(super) fn remove_array_value(text: &str, pointer: &str, wanted: &Value) -> Option<String> {
    remove_matching(text, pointer, |value| value == wanted)
}

pub(super) fn remove_hook_command(text: &str, event: &str, command: &str) -> Option<String> {
    let pointer = format!("/hooks/{event}");
    remove_matching(text, &pointer, |entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.contains(command))
                })
            })
    })
}

fn remove_matching(text: &str, pointer: &str, matches: impl Fn(&Value) -> bool) -> Option<String> {
    let keys: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let (start, end) = pointer_span(text, &keys)?;
    let items = array_items(text, start, end)?;
    let (index, &(item_start, item_end)) = items.iter().enumerate().find(|(_, (lo, hi))| {
        serde_json::from_str::<Value>(&text[*lo..*hi])
            .ok()
            .is_some_and(|value| matches(&value))
    })?;
    let (delete_start, delete_end) = if index + 1 < items.len() {
        let comma = find_byte_forward(text, item_end, items[index + 1].0, b',')?;
        (item_start, comma + 1)
    } else if index > 0 {
        let comma = find_byte_backward(text, items[index - 1].1, item_start, b',')?;
        (comma, item_end)
    } else {
        (item_start, item_end)
    };
    let mut output = text.to_string();
    output.replace_range(delete_start..delete_end, "");
    Some(output)
}

fn pointer_span(text: &str, keys: &[&str]) -> Option<(usize, usize)> {
    let mut span = root_span(text)?;
    for key in keys {
        span = object_member(text, span, key)?;
    }
    Some(span)
}

fn root_span(text: &str) -> Option<(usize, usize)> {
    let start = skip_ws(text, 0);
    Some((start, scan_value(text, start)?))
}

fn object_member(text: &str, object: (usize, usize), wanted: &str) -> Option<(usize, usize)> {
    object_members(text, object)?
        .into_iter()
        .find(|member| member.key == wanted)
        .map(|member| (member.value_start, member.value_end))
}

struct ObjectMember {
    key: String,
    member_start: usize,
    value_start: usize,
    value_end: usize,
}

fn object_members(text: &str, object: (usize, usize)) -> Option<Vec<ObjectMember>> {
    if !matches!(text.as_bytes().get(object.0), Some(b'{')) {
        return None;
    }
    let mut members = Vec::new();
    let mut cursor = skip_ws(text, object.0 + 1);
    while cursor < object.1 && text.as_bytes()[cursor] != b'}' {
        let member_start = cursor;
        let key_end = scan_string(text, cursor)?;
        let key: String = serde_json::from_str(&text[cursor..key_end]).ok()?;
        cursor = skip_ws(text, key_end);
        if text.as_bytes().get(cursor) != Some(&b':') {
            return None;
        }
        let value_start = skip_ws(text, cursor + 1);
        let value_end = scan_value(text, value_start)?;
        members.push(ObjectMember {
            key,
            member_start,
            value_start,
            value_end,
        });
        cursor = skip_ws(text, value_end);
        match text.as_bytes().get(cursor) {
            Some(b',') => cursor = skip_ws(text, cursor + 1),
            Some(b'}') => break,
            _ => return None,
        }
    }
    Some(members)
}

fn insert_object_member(
    text: &str,
    object: (usize, usize),
    key: &str,
    value: &str,
) -> Option<String> {
    let close = object.1.checked_sub(1)?;
    if text.as_bytes().get(close) != Some(&b'}') {
        return None;
    }
    let occupied = !text[object.0 + 1..close].trim().is_empty();
    let insertion = format!(
        "{}{}:{value}",
        if occupied { "," } else { "" },
        serde_json::to_string(key).ok()?
    );
    let mut output = text.to_string();
    output.insert_str(close, &insertion);
    Some(output)
}

fn array_items(text: &str, start: usize, end: usize) -> Option<Vec<(usize, usize)>> {
    if text.as_bytes().get(start) != Some(&b'[') {
        return None;
    }
    let mut items = Vec::new();
    let mut cursor = skip_ws(text, start + 1);
    while cursor < end && text.as_bytes()[cursor] != b']' {
        let item_end = scan_value(text, cursor)?;
        items.push((cursor, item_end));
        cursor = skip_ws(text, item_end);
        match text.as_bytes().get(cursor) {
            Some(b',') => cursor = skip_ws(text, cursor + 1),
            Some(b']') => break,
            _ => return None,
        }
    }
    Some(items)
}

fn scan_value(text: &str, start: usize) -> Option<usize> {
    match *text.as_bytes().get(start)? {
        b'"' => scan_string(text, start),
        b'{' | b'[' => {
            let open = text.as_bytes()[start];
            let close = if open == b'{' { b'}' } else { b']' };
            let mut depth = 1usize;
            let mut cursor = start + 1;
            while cursor < text.len() {
                match text.as_bytes()[cursor] {
                    b'"' => cursor = scan_string(text, cursor)?,
                    byte if byte == open => {
                        depth += 1;
                        cursor += 1;
                    }
                    byte if byte == close => {
                        depth -= 1;
                        cursor += 1;
                        if depth == 0 {
                            return Some(cursor);
                        }
                    }
                    _ => cursor += 1,
                }
            }
            None
        }
        _ => {
            let mut cursor = start;
            while cursor < text.len()
                && !matches!(
                    text.as_bytes()[cursor],
                    b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t'
                )
            {
                cursor += 1;
            }
            (cursor > start).then_some(cursor)
        }
    }
}

fn scan_string(text: &str, start: usize) -> Option<usize> {
    if text.as_bytes().get(start) != Some(&b'"') {
        return None;
    }
    let mut cursor = start + 1;
    while cursor < text.len() {
        match text.as_bytes()[cursor] {
            b'\\' => cursor += 2,
            b'"' => return Some(cursor + 1),
            _ => cursor += 1,
        }
    }
    None
}

fn skip_ws(text: &str, mut cursor: usize) -> usize {
    while cursor < text.len() && text.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    cursor
}

fn find_byte_forward(text: &str, start: usize, end: usize, wanted: u8) -> Option<usize> {
    (start..end).find(|&index| text.as_bytes()[index] == wanted)
}

fn find_byte_backward(text: &str, start: usize, end: usize, wanted: u8) -> Option<usize> {
    (start..end)
        .rev()
        .find(|&index| text.as_bytes()[index] == wanted)
}
