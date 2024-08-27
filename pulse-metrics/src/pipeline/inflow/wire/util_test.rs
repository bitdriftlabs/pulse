// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;

// Test for an empty buffer
#[test]
fn test_process_buffer_newlines_empty() {
  let mut buffer = BytesMut::new();
  buffer.extend_from_slice(b"");
  let result = process_buffer_newlines(&mut buffer, true);
  assert_eq!(0, result.len());
}

// Test for a single newline in the buffer
#[test]
fn test_process_buffer_newlines_empty_single() {
  let mut buffer = BytesMut::new();
  buffer.extend_from_slice(b"\n");
  let result = process_buffer_newlines(&mut buffer, true);
  assert_eq!(1, result.len());
}

// Test for multiple newlines with no content in between, a mix of LF and CR+LF
#[test]
fn test_process_buffer_newlines_empty_multi() {
  let mut buffer = BytesMut::new();
  buffer.extend_from_slice(b"\r\n\n\r\n\n");
  let result = process_buffer_newlines(&mut buffer, true);
  assert_eq!(4, result.len());
}

// Test for multiple newlines with content in between, a mix of LF and CR+LF
#[test]
fn test_process_buffer_newlines_content_multi() {
  let mut buffer = BytesMut::new();
  buffer.extend_from_slice(b"line 1\r\nline 2\nline 3\r\nline 4\n");
  let result = process_buffer_newlines(&mut buffer, true);
  assert_eq!(4, result.len());
  assert_eq!("line 1", result[0]);
  assert_eq!("line 2", result[1]);
  assert_eq!("line 3", result[2]);
  assert_eq!("line 4", result[3]);
}
