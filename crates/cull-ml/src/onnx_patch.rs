//! Minimal protobuf rewriting of an ONNX model: turn fixed dimensions of the
//! declared graph inputs/outputs into symbolic ones.
//!
//! YuNet 2023mar is fully convolutional (its reshapes use `-1`, its resizes
//! use scales) but the published ONNX declares a fixed `1x3x640x640` input,
//! which ONNX Runtime enforces. Declaring H/W symbolic lets us run it at the
//! photo's aspect ratio (640x448 for 3:2 instead of a letterboxed 640x640)
//! and at 1280 px for group shots. Weights and nodes are untouched; the
//! checksum is verified on the original file.

use crate::{MlError, Result};

fn err(msg: &str) -> MlError {
    MlError::Runtime(format!("onnx patch: {msg}"))
}

fn read_varint(buf: &[u8], pos: &mut usize) -> Result<u64> {
    let mut v: u64 = 0;
    for shift in (0..64).step_by(7) {
        let b = *buf.get(*pos).ok_or_else(|| err("truncated varint"))?;
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(err("varint too long"))
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// One field of a protobuf message: number, wire type, and the byte range of
/// the whole field (key included) plus its payload range for wire type 2.
struct Field {
    number: u64,
    wire: u8,
    whole: std::ops::Range<usize>,
    payload: std::ops::Range<usize>,
}

fn fields(buf: &[u8]) -> Result<Vec<Field>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let start = pos;
        let key = read_varint(buf, &mut pos)?;
        let wire = (key & 7) as u8;
        let number = key >> 3;
        let payload_start;
        match wire {
            0 => {
                read_varint(buf, &mut pos)?;
                payload_start = pos;
            }
            1 => {
                payload_start = pos;
                pos += 8;
            }
            2 => {
                let len = usize::try_from(read_varint(buf, &mut pos)?).map_err(|_| err("length"))?;
                payload_start = pos;
                pos = pos.checked_add(len).ok_or_else(|| err("length overflow"))?;
            }
            5 => {
                payload_start = pos;
                pos += 4;
            }
            _ => return Err(err("unsupported wire type")),
        }
        if pos > buf.len() {
            return Err(err("truncated field"));
        }
        out.push(Field { number, wire, whole: start..pos, payload: payload_start..pos });
    }
    Ok(out)
}

fn write_len_field(out: &mut Vec<u8>, number: u64, payload: &[u8]) {
    write_varint(out, (number << 3) | 2);
    write_varint(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

/// Copy `msg`, replacing the payload of every length-delimited field
/// `number` by `f(index, payload)` (index among the fields with that number).
/// Rewrites the payload of one length-delimited field.
type FieldMap<'a> = dyn FnMut(usize, &[u8]) -> Result<Vec<u8>> + 'a;

fn map_field(msg: &[u8], number: u64, f: &mut FieldMap<'_>) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(msg.len() + 64);
    let mut index = 0;
    for fld in fields(msg)? {
        if fld.number == number && fld.wire == 2 {
            let new = f(index, &msg[fld.payload.clone()])?;
            index += 1;
            write_len_field(&mut out, number, &new);
        } else {
            out.extend_from_slice(&msg[fld.whole.clone()]);
        }
    }
    Ok(out)
}

fn string_field(msg: &[u8], number: u64) -> Option<String> {
    let flds = fields(msg).ok()?;
    let f = flds.iter().find(|f| f.number == number && f.wire == 2)?;
    String::from_utf8(msg[f.payload.clone()].to_vec()).ok()
}

/// `TensorShapeProto` with the dimensions selected by `param(dim_index)`
/// replaced by `dim_param`.
fn patch_shape(shape: &[u8], param: &dyn Fn(usize) -> Option<&'static str>) -> Result<Vec<u8>> {
    // TensorShapeProto.dim = 1; Dimension.dim_param = 2.
    map_field(shape, 1, &mut |i, dim| match param(i) {
        Some(name) => {
            let mut d = Vec::new();
            write_len_field(&mut d, 2, name.as_bytes());
            Ok(d)
        }
        None => Ok(dim.to_vec()),
    })
}

fn patch_value_info(vi: &[u8], param: &dyn Fn(usize) -> Option<&'static str>) -> Result<Vec<u8>> {
    // ValueInfoProto.type = 2 -> TypeProto.tensor_type = 1 -> Tensor.shape = 2.
    map_field(vi, 2, &mut |_, ty| {
        map_field(ty, 1, &mut |_, tt| map_field(tt, 2, &mut |_, shape| patch_shape(shape, param)))
    })
}

/// Make the declared H/W of graph input `input_name` (NCHW) and dimension 1
/// of every graph output symbolic.
pub(crate) fn make_spatial_dims_dynamic(model: &[u8], input_name: &str) -> Result<Vec<u8>> {
    let input_param = |i: usize| match i {
        2 => Some("height"),
        3 => Some("width"),
        _ => None,
    };
    let output_param = |i: usize| (i == 1).then_some("anchors");
    let mut patched_inputs = 0;
    // ModelProto.graph = 7 -> GraphProto.input = 11, output = 12.
    let out = map_field(model, 7, &mut |_, graph| {
        let g = map_field(graph, 11, &mut |_, vi| {
            if string_field(vi, 1).as_deref() == Some(input_name) {
                patched_inputs += 1;
                patch_value_info(vi, &input_param)
            } else {
                Ok(vi.to_vec())
            }
        })?;
        map_field(&g, 12, &mut |_, vi| patch_value_info(vi, &output_param))
    })?;
    if patched_inputs != 1 {
        return Err(err("model input not found"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        for v in [0u64, 1, 127, 128, 300, 640, u32::MAX as u64, u64::MAX] {
            let mut b = Vec::new();
            write_varint(&mut b, v);
            let mut p = 0;
            assert_eq!(read_varint(&b, &mut p).unwrap(), v);
            assert_eq!(p, b.len());
        }
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(make_spatial_dims_dynamic(&[0xff, 0xff, 0xff], "input").is_err());
        assert!(make_spatial_dims_dynamic(&[], "input").is_err());
        assert!(make_spatial_dims_dynamic(&[0x3a, 0x05, 0x01], "input").is_err());
    }

    #[test]
    fn patches_a_synthetic_value_info() {
        // Dimension{dim_value: 640}
        let mut dim = Vec::new();
        write_varint(&mut dim, 1 << 3);
        write_varint(&mut dim, 640);
        let mut shape = Vec::new();
        for _ in 0..4 {
            write_len_field(&mut shape, 1, &dim);
        }
        let mut tensor = Vec::new();
        write_len_field(&mut tensor, 2, &shape);
        let mut ty = Vec::new();
        write_len_field(&mut ty, 1, &tensor);
        let mut vi = Vec::new();
        write_len_field(&mut vi, 1, b"input");
        write_len_field(&mut vi, 2, &ty);
        let mut graph = Vec::new();
        write_len_field(&mut graph, 11, &vi);
        let mut model = Vec::new();
        write_varint(&mut model, 1 << 3); // ir_version = 7
        write_varint(&mut model, 7);
        write_len_field(&mut model, 7, &graph);
        let out = make_spatial_dims_dynamic(&model, "input").unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("height") && s.contains("width"));
        // Two dims keep their value (N, C), two became symbolic.
        assert_eq!(out.windows(2).filter(|w| w == &[0x08, 0x80]).count(), 2);
    }
}
