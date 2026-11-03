//! FIRA canonical JSON writer for the `fira-dih/1` determinism hash
//! (report-provenance-slice, design §3.4).
//!
//! Converts an in-memory `serde_json::Value` tree into canonical UTF-8 bytes
//! using FIRA's own rules. `serde_json` is used only as the tree; its printer,
//! map ordering (with or without `preserve_order`), whitespace and escaping are
//! never used for the output:
//!
//! - O-1 objects: members sorted ascending by the raw, unescaped UTF-8 bytes of
//!   the key, written as `"key":value`, joined by `,`.
//! - A-1 arrays: element order preserved, joined by `,`.
//! - L-1 literals: `true`, `false`, `null`.
//! - N-1 numbers: integers representable as `u64`/`i64` only, base-10; any
//!   other number is rejected.
//! - S-1 strings: the exact escape table of design §3.4.
//!
//! No whitespace is emitted anywhere.

use serde_json::{Number, Value};

/// Error produced when a value cannot be written canonically (rule N-1: a
/// non-integer number, or one not representable as `u64`/`i64`).
///
/// Private to the provenance module; mapped to
/// `ProvenanceError::Canonicalization` by the parent module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CanonicalError(pub(super) String);

/// Canonical UTF-8 bytes of `value` per design §3.4.
pub(super) fn to_canonical_bytes(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = String::new();
    write_value(value, &mut out)?;
    Ok(out.into_bytes())
}

fn write_value(value: &Value, out: &mut String) -> Result<(), CanonicalError> {
    match value {
        // L-1
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        // N-1
        Value::Number(n) => write_number(n, out)?,
        // S-1
        Value::String(s) => write_string(s, out),
        // A-1: order preserved.
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        // O-1: sorted by the raw, unescaped key bytes, independent of the
        // map's internal order.
        Value::Object(map) => {
            let mut members: Vec<(&String, &Value)> = map.iter().collect();
            members.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push('{');
            for (i, (key, member)) in members.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write_value(member, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// N-1: base-10 integers only (`-` for negatives; no `+`, leading zeros,
/// exponent or fraction — the `Display` form of Rust integers).
fn write_number(n: &Number, out: &mut String) -> Result<(), CanonicalError> {
    if let Some(u) = n.as_u64() {
        out.push_str(&u.to_string());
        Ok(())
    } else if let Some(i) = n.as_i64() {
        out.push_str(&i.to_string());
        Ok(())
    } else {
        Err(CanonicalError(format!(
            "non-integer number `{n}` cannot be written canonically (fira-dih/1 N-1)"
        )))
    }
}

/// S-1: `"` + escaped content + `"`, escaping per Unicode scalar value.
fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Other U+0000–U+001F: `\u00` + 2 lowercase hex digits.
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            // Everything else (incl. `/`, U+007F, all non-ASCII): raw UTF-8.
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::{to_canonical_bytes, CanonicalError};
    use serde_json::{json, Map, Value};

    fn canon(v: &Value) -> Vec<u8> {
        to_canonical_bytes(v).expect("canonicalizable")
    }

    fn string_bytes(s: &str) -> Vec<u8> {
        canon(&Value::String(s.to_string()))
    }

    // ---- O-1 objects -----------------------------------------------------

    #[test]
    fn object_members_sorted_by_raw_key_bytes() {
        // Bytewise: 'Z' (0x5A) < '_' (0x5F) < 'a' (0x61) < "aa" < 'b' < 'é' (0xC3..).
        let v = json!({"b": 1, "é": 6, "aa": 4, "_": 2, "a": 3, "Z": 0});
        assert_eq!(
            canon(&v),
            r#"{"Z":0,"_":2,"a":3,"aa":4,"b":1,"é":6}"#.as_bytes().to_vec()
        );
    }

    #[test]
    fn object_order_independent_of_insertion_order() {
        let mut forward = Map::new();
        forward.insert("alpha".into(), json!(1));
        forward.insert("beta".into(), json!(2));
        forward.insert("gamma".into(), json!(3));
        let mut reverse = Map::new();
        reverse.insert("gamma".into(), json!(3));
        reverse.insert("beta".into(), json!(2));
        reverse.insert("alpha".into(), json!(1));
        let expected = br#"{"alpha":1,"beta":2,"gamma":3}"#.to_vec();
        assert_eq!(canon(&Value::Object(forward)), expected);
        assert_eq!(canon(&Value::Object(reverse)), expected);
    }

    #[test]
    fn keys_sorted_before_escaping() {
        // Raw '"' (0x22) sorts before '#' (0x23); its escaped form `\"` would
        // start with '\' (0x5C) and sort after '#'. The raw rule wins.
        let v = json!({"#": 2, "\"": 1});
        assert_eq!(canon(&v), br##"{"\"":1,"#":2}"##.to_vec());
    }

    #[test]
    fn envelope_member_order() {
        // design §3.2: after sorting, members are always in this order.
        let v = json!({
            "scheme": "fira-dih/1",
            "gates": [],
            "findings": [],
            "expectation_violations": []
        });
        assert_eq!(
            canon(&v),
            br#"{"expectation_violations":[],"findings":[],"gates":[],"scheme":"fira-dih/1"}"#
                .to_vec()
        );
    }

    #[test]
    fn empty_containers() {
        assert_eq!(canon(&json!({})), b"{}".to_vec());
        assert_eq!(canon(&json!([])), b"[]".to_vec());
        assert_eq!(canon(&json!("")), b"\"\"".to_vec());
    }

    // ---- A-1 arrays / L-1 literals ---------------------------------------

    #[test]
    fn arrays_preserve_order_and_nest() {
        assert_eq!(canon(&json!([3, 1, 2])), b"[3,1,2]".to_vec());
        let v = json!([[1, [2, []]], [], "x", {"k": [true, false, null]}]);
        assert_eq!(
            canon(&v),
            br#"[[1,[2,[]]],[],"x",{"k":[true,false,null]}]"#.to_vec()
        );
    }

    #[test]
    fn literals() {
        assert_eq!(canon(&json!(true)), b"true".to_vec());
        assert_eq!(canon(&json!(false)), b"false".to_vec());
        assert_eq!(canon(&Value::Null), b"null".to_vec());
    }

    // ---- N-1 numbers -----------------------------------------------------

    #[test]
    fn integers_base10() {
        assert_eq!(canon(&json!(0)), b"0".to_vec());
        assert_eq!(canon(&json!(-1)), b"-1".to_vec());
        assert_eq!(canon(&json!(42)), b"42".to_vec());
        assert_eq!(canon(&json!(i64::MIN)), b"-9223372036854775808".to_vec());
        assert_eq!(canon(&json!(u64::MAX)), b"18446744073709551615".to_vec());
    }

    #[test]
    fn floats_rejected() {
        for v in [json!(1.5), json!(1.0), json!(-0.5)] {
            let err: CanonicalError = to_canonical_bytes(&v).unwrap_err();
            assert!(err.0.contains("N-1"), "unexpected message: {}", err.0);
        }
    }

    #[test]
    fn nested_float_rejected() {
        assert!(to_canonical_bytes(&json!([1, {"a": [2, 0.25]}])).is_err());
    }

    // ---- S-1 strings -----------------------------------------------------

    #[test]
    fn escape_table_rows() {
        let rows: [(&str, &[u8]); 10] = [
            ("\"", br#""\"""#),
            ("\\", br#""\\""#),
            ("\u{0008}", br#""\b""#),
            ("\u{000C}", br#""\f""#),
            ("\n", br#""\n""#),
            ("\r", br#""\r""#),
            ("\t", br#""\t""#),
            ("\u{0000}", br#""\u0000""#),
            ("\u{0001}", br#""\u0001""#),
            ("\u{001F}", br#""\u001f""#),
        ];
        for (input, expected) in rows {
            assert_eq!(
                string_bytes(input),
                expected.to_vec(),
                "input U+{:04X}",
                input.chars().next().unwrap() as u32
            );
        }
    }

    #[test]
    fn unescaped_scalars_written_raw() {
        // `/`, U+007F and U+0020 are not escaped.
        assert_eq!(string_bytes("a/b"), b"\"a/b\"".to_vec());
        assert_eq!(string_bytes("\u{007F}"), vec![b'"', 0x7F, b'"']);
        assert_eq!(string_bytes(" "), b"\" \"".to_vec());
    }

    #[test]
    fn non_ascii_written_as_raw_utf8() {
        // é = C3 A9; € = E2 82 AC; U+1F600 = F0 9F 98 80.
        assert_eq!(
            string_bytes("é€\u{1F600}"),
            vec![b'"', 0xC3, 0xA9, 0xE2, 0x82, 0xAC, 0xF0, 0x9F, 0x98, 0x80, b'"']
        );
    }

    #[test]
    fn mixed_string() {
        assert_eq!(
            string_bytes("say \"hi\"\n\tpath/C:\\x"),
            br#""say \"hi\"\n\tpath/C:\\x""#.to_vec()
        );
    }

    // ---- determinism -----------------------------------------------------

    #[test]
    fn deterministic() {
        let v = json!({"z": [1, "two", {"y": null, "x": false}], "a": -3});
        assert_eq!(canon(&v), canon(&v));
        assert_eq!(
            canon(&v),
            br#"{"a":-3,"z":[1,"two",{"x":false,"y":null}]}"#.to_vec()
        );
    }
}
