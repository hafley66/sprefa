use crate::ast::Value;

use super::SourceStageError;

const MAGIC: &[u8; 8] = b"SPRFVAL\0";
const VERSION: u16 = 1;

pub(in crate::engine::pipeline) fn encode_values(
    values: &[Value],
) -> Result<Vec<u8>, SourceStageError> {
    let count = u32::try_from(values.len()).map_err(|_| SourceStageError::EncodingTooLarge)?;
    let mut out = Vec::with_capacity(14 + values.len() * 9);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    for value in values {
        match value {
            Value::Null => out.push(0),
            Value::Text(text) => {
                out.push(1);
                let len =
                    u32::try_from(text.len()).map_err(|_| SourceStageError::EncodingTooLarge)?;
                out.extend_from_slice(&len.to_be_bytes());
                out.extend_from_slice(text.as_bytes());
            }
            Value::Int(value) => {
                out.push(2);
                out.extend_from_slice(&value.to_be_bytes());
            }
        }
    }
    Ok(out)
}

pub(in crate::engine::pipeline) fn decode_values(
    bytes: &[u8],
) -> Result<Vec<Value>, SourceStageError> {
    let mut cursor = Cursor::new(bytes);
    if cursor.take(8)? != MAGIC || cursor.u16()? != VERSION {
        return Err(SourceStageError::BadCodec);
    }
    let count = cursor.u32()? as usize;
    if count > cursor.remaining() {
        return Err(SourceStageError::BadCodec);
    }
    let mut values = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        values.push(match cursor.byte()? {
            0 => Value::Null,
            1 => {
                let len = cursor.u32()? as usize;
                let text = std::str::from_utf8(cursor.take(len)?)
                    .map_err(|_| SourceStageError::BadCodec)?;
                Value::Text(text.into())
            }
            2 => Value::Int(i64::from_be_bytes(cursor.array()?)),
            _ => return Err(SourceStageError::BadCodec),
        });
    }
    if cursor.remaining() != 0 {
        return Err(SourceStageError::BadCodec);
    }
    Ok(values)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], SourceStageError> {
        let end = self.at.checked_add(len).ok_or(SourceStageError::BadCodec)?;
        let result = self
            .bytes
            .get(self.at..end)
            .ok_or(SourceStageError::BadCodec)?;
        self.at = end;
        Ok(result)
    }

    fn byte(&mut self) -> Result<u8, SourceStageError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, SourceStageError> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, SourceStageError> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], SourceStageError> {
        self.take(N)?
            .try_into()
            .map_err(|_| SourceStageError::BadCodec)
    }
}
