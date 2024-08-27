// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./fst_test.rs"]
mod fst_test;

use super::poll_filter::PollFilterProvider;
use bytes::Bytes;
use fst::{Set, Streamer};
use parking_lot::RwLock;

type Fst = Set<Bytes>;

//
// FstFilterProvider
//

// Searches for a name/string inside an FST.
pub struct FstFilterProvider {
  fst: RwLock<Fst>,
}

impl FstFilterProvider {
  pub async fn new(file: Bytes) -> Result<Self, anyhow::Error> {
    Ok(Self {
      fst: RwLock::new(Self::construct_fst(file).await?),
    })
  }

  // Constructs a FST from a source file.
  async fn construct_fst(file: Bytes) -> anyhow::Result<Fst> {
    let fst = Set::new(file)?;

    // Verifying the FST is a potentially expensive computation.
    let fst = tokio::task::spawn_blocking(move || {
      fst.as_fst().verify()?;
      Ok::<Fst, fst::Error>(fst)
    })
    .await
    .unwrap()?;

    Ok(fst)
  }
}

#[async_trait::async_trait]
impl PollFilterProvider for FstFilterProvider {
  async fn update(&self, file: Bytes) -> Result<(), anyhow::Error> {
    let new_fst = Self::construct_fst(file).await?;
    *self.fst.write() = new_fst;
    Ok(())
  }

  fn is_match(&self, name: &[u8]) -> bool {
    self.fst.read().contains(name)
  }

  fn dump(&self) -> Vec<u8> {
    let mut output = Vec::new();
    let fst = self.fst.read();
    let mut stream = fst.stream();
    while let Some(key) = stream.next() {
      output.extend(key);
      output.push(b'\n');
    }

    output
  }

  fn set_size(&self) -> usize {
    self.fst.read().len()
  }
}
