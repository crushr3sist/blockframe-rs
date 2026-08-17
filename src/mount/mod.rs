// Copyright © 2026 Ronny (2Fresh)
// SPDX-License-Identifier: Apache-2.0

pub mod cache;
pub mod source;

#[cfg(unix)]
mod filesystem_unix;
#[cfg(unix)]
pub use filesystem_unix::BlockframeFS;

#[cfg(windows)]
mod filesystem_win;
#[cfg(windows)]
pub use filesystem_win::BlockframeFS;
