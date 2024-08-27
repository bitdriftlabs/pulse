// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./regex_test.rs"]
mod regex_test;

use super::poll_filter::PollFilterProvider;
use bytes::{Buf, Bytes};
use parking_lot::RwLock;
use regex::bytes::{RegexSet, RegexSetBuilder};
use std::io::BufRead;

//
// RegexFilterProvider
//

// Searches for a name/string against a regex set.
pub struct RegexFilterProvider {
  regex_set: RwLock<RegexSet>,
}

impl RegexFilterProvider {
  pub async fn new(file: Bytes) -> Result<Self, anyhow::Error> {
    Ok(Self {
      regex_set: RwLock::new(Self::construct_regex_set(file).await?),
    })
  }

  async fn construct_regex_set(file: Bytes) -> anyhow::Result<RegexSet> {
    let regex_set = tokio::task::spawn_blocking(|| {
      let lines = std::io::BufReader::new(file.reader())
        .lines()
        .map_while(Result::ok);
      Self::build_regex_set(lines)
    })
    .await
    .unwrap()?;
    Ok(regex_set)
  }

  pub fn build_regex_set<T: AsRef<str>, I: IntoIterator<Item = T>>(
    patterns: I,
  ) -> anyhow::Result<RegexSet> {
    let r = RegexSetBuilder::new(patterns).unicode(false).build()?;
    Ok(r)
  }
}

#[async_trait::async_trait]
impl PollFilterProvider for RegexFilterProvider {
  async fn update(&self, file: Bytes) -> Result<(), anyhow::Error> {
    let new_regex_set = Self::construct_regex_set(file).await?;
    *self.regex_set.write() = new_regex_set;
    Ok(())
  }

  fn is_match(&self, name: &[u8]) -> bool {
    self.regex_set.read().is_match(name)
  }

  fn dump(&self) -> Vec<u8> {
    let mut output = Vec::new();
    for regex in self.regex_set.read().patterns() {
      output.extend(regex.as_bytes());
      output.push(b'\n');
    }

    output
  }

  fn set_size(&self) -> usize {
    self.regex_set.read().len()
  }
}
