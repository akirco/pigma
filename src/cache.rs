//! On-disk media cache (`CacheManager`) and its audio/cover/lyrics/index
//! submodules. Navigation content caching lives in [`crate::service::ContentCache`].

mod audio;
pub(crate) mod cover;
mod index;
mod lyrics;
mod manager;

pub use audio::CacheFileProvider;
pub(crate) use audio::canonical_ext;
pub use manager::*;
