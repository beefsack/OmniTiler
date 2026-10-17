#![forbid(unsafe_code)]
//! Pager miniature icons: portable sizing, cache, and queues.
//!
//! The painter stays an opaque GDI surface (`crate::ffi`): icons are
//! worker-produced owned pixel buffers, already flattened onto the miniature
//! gray, so paint only blits opaque bytes with `StretchDIBits(SRCCOPY)`.
//! There is no alpha channel in the overlay and no cross-thread `HICON`:
//! icon translucency is resolved once at lookup time, not per paint.
//!
//! [`IconImage`] pixels are top-down rows of BGRA bytes at `ICON_RENDER_PX`
//! square, filled with [`MINI_GRAY`] where the artwork is transparent.
//! Identity per miniature travels in the frame (see `crate::pager`), keyed
//! by the full [`WindowKey`](crate::workspace::WindowKey) lifetime so a
//! recycled HWND never inherits another window's artwork.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tiler_core::geometry::Rect;

use crate::workspace::WindowKey;

/// Maximum icon side at 96 DPI (logical px): caps artwork so the miniature
/// outline stays visible around it instead of filling the whole cell.
pub const ICON_MAX_96: i32 = 16;
/// Minimum legible icon side at 96 DPI (logical px): smaller artwork is
/// unreadable, so the miniature omits the icon and keeps the plain rectangle.
pub const ICON_MIN_96: i32 = 8;
/// Miniature border kept clear of artwork (physical px, never scaled): keeps
/// the 1px painted border visible at any DPI.
pub const MINI_BORDER_PX: i32 = 1;
/// Extra padding between the miniature border and the artwork (physical px,
/// never scaled): keeps the border distinct from the icon at any DPI.
pub const MINI_PAD_PX: i32 = 1;
/// Worker render size: icons are extracted once at this square size and
/// downscaled by the blit. Large enough for the 16-logical-px cap at 200%.
pub const ICON_RENDER_PX: u32 = 32;
/// Miniature fill the worker flattens artwork onto (matches the painter).
pub const MINI_GRAY: (u8, u8, u8) = (168, 168, 168);
/// Negative (missing/unreadable) entries retry after this age: apps often set
/// their icons after window creation, so a miss is re-looked-up promptly.
pub const NEG_RETRY: Duration = Duration::from_secs(5);
/// Positive entries refresh after this age: there is no Windows event for
/// icon changes, so periodic re-lookup picks up `WM_SETICON` updates.
pub const POS_REFRESH: Duration = Duration::from_secs(10);

/// One worker-rendered icon: opaque top-down BGRA bytes (`width * height *
/// 4`), artwork alpha already flattened onto [`MINI_GRAY`]. The final blit
/// ignores the alpha byte (`SRCCOPY`), so every byte here is paint-ready.
/// `digest` is a stable full-content FNV-1a over dimensions plus every byte,
/// computed once at construction so the per-pump repaint hash mixes one word
/// instead of rewalking 4KB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub digest: u64,
}

impl IconImage {
    /// Build one image, stamping its content digest.
    #[must_use]
    pub fn new(width: u32, height: u32, bgra: Vec<u8>) -> Self {
        let digest = icon_digest(width, height, &bgra);
        Self {
            width,
            height,
            bgra,
            digest,
        }
    }

    /// True when the buffer shape matches its dimensions. The 256px cap
    /// rejects corrupt oversized buffers before they reach the painter.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.width <= 256
            && self.height <= 256
            && self.bgra.len() == self.width as usize * self.height as usize * 4
    }
}

fn icon_digest(width: u32, height: u32, bgra: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for word in [u64::from(width), u64::from(height), bgra.len() as u64] {
        hash ^= word;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for byte in bgra {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Where one artwork result came from (bounded log vocabulary, no identities).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconSource {
    Window,
    Class,
    Exe,
    Miss,
    Refused,
}

impl IconSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Class => "class",
            Self::Exe => "exe",
            Self::Miss => "miss",
            Self::Refused => "refused",
        }
    }
}

/// Centered icon rectangle inside one miniature, or `None` when the
/// miniature cannot fit a legible icon: the usable area (miniature minus the
/// 1px border plus 1px padding on every side) must clear the DPI-scaled
/// minimum, and the artwork caps at the DPI-scaled maximum. Missing icons
/// leave the existing rectangle unchanged.
#[must_use]
pub fn icon_rect_for_mini(mini: Rect, dpi: u32) -> Option<Rect> {
    let inset = MINI_BORDER_PX + MINI_PAD_PX;
    let avail_w = mini.w - inset.saturating_mul(2);
    let avail_h = mini.h - inset.saturating_mul(2);
    let min_side = crate::pager::dpi_scale(ICON_MIN_96, dpi);
    let max_side = crate::pager::dpi_scale(ICON_MAX_96, dpi);
    if avail_w < min_side || avail_h < min_side {
        return None;
    }
    let side = max_side.min(avail_w).min(avail_h);
    Some(Rect {
        x: mini.x + (mini.w - side) / 2,
        y: mini.y + (mini.h - side) / 2,
        w: side,
        h: side,
    })
}

#[derive(Debug)]
struct CacheEntry {
    image: Option<Arc<IconImage>>,
    updated: Instant,
}

/// Identity-keyed icon cache: one entry per full [`WindowKey`] lifetime.
/// Pruned to live membership every pump, so closed windows and recycled
/// HWNDs (new pid/creation, hence a different key) never reuse artwork.
#[derive(Debug, Default)]
pub struct PagerIconCache {
    entries: HashMap<WindowKey, CacheEntry>,
}

impl PagerIconCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Hit only: artwork bytes for `key`, if a successful lookup landed.
    #[must_use]
    pub fn get(&self, key: &WindowKey) -> Option<Arc<IconImage>> {
        self.entries.get(key).and_then(|entry| entry.image.clone())
    }

    /// Store one worker result (hit or miss) stamped at `now`.
    pub fn insert(&mut self, key: WindowKey, image: Option<Arc<IconImage>>, now: Instant) {
        self.entries.insert(
            key,
            CacheEntry {
                image,
                updated: now,
            },
        );
    }

    /// Drop every entry whose key is no longer live (closed/departed).
    /// Returns the number of pruned entries.
    pub fn prune(&mut self, live: &HashSet<WindowKey>) -> usize {
        let before = self.entries.len();
        self.entries.retain(|key, _| live.contains(key));
        before - self.entries.len()
    }

    /// Keys needing a worker lookup now: unknown keys, negative entries
    /// older than [`NEG_RETRY`], and positive entries older than
    /// [`POS_REFRESH`]. Uncapped: the caller only passes drawable live keys
    /// and the channel dedupes them, so queued work never exceeds one entry
    /// per distinct passed key.
    #[must_use]
    pub fn requests(
        &self,
        keys: impl IntoIterator<Item = WindowKey>,
        now: Instant,
    ) -> Vec<WindowKey> {
        let mut out = Vec::new();
        for key in keys {
            match self.entries.get(&key) {
                None => out.push(key),
                Some(entry) => {
                    let age = now.saturating_duration_since(entry.updated);
                    let stale = if entry.image.is_some() {
                        age >= POS_REFRESH
                    } else {
                        age >= NEG_RETRY
                    };
                    if stale {
                        out.push(key);
                    }
                }
            }
        }
        out
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One finished worker lookup (hit or miss). `generation` is the channel
/// generation at pop time: results from a pre-suppression lookup are
/// dropped on push, so a late result can never clobber a newer request.
#[derive(Debug, Clone)]
pub struct IconResult {
    pub key: WindowKey,
    pub image: Option<Arc<IconImage>>,
    pub source: IconSource,
    pub generation: u64,
}

/// Unbounded request/result queues between the tile loop and the single
/// background worker. Requests dedupe against both the queued set and the
/// outstanding set (popped but result not yet drained), so queued work never
/// exceeds one entry per distinct drawable key. No strict live-count claim:
/// an already-started lookup may finish after its key departs (its result
/// still passes the generation and liveness gates); the single worker holds
/// at most one in-flight pop, and results drain every pump. `clear`
/// (suppression) bumps the generation: late pre-clear results are refused
/// at push.
#[derive(Debug, Default)]
pub struct IconChannel {
    pending: VecDeque<WindowKey>,
    outstanding: HashSet<WindowKey>,
    results: VecDeque<IconResult>,
    generation: u64,
}

impl IconChannel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            outstanding: HashSet::new(),
            results: VecDeque::new(),
            generation: 0,
        }
    }

    /// Queue one request; false when already queued or already outstanding.
    pub fn push_request(&mut self, key: WindowKey) -> bool {
        if self.pending.contains(&key) || self.outstanding.contains(&key) {
            return false;
        }
        self.pending.push_back(key);
        true
    }

    /// Pop one request for the worker, tracking it as outstanding until its
    /// result is drained.
    pub fn pop_request(&mut self) -> Option<(WindowKey, u64)> {
        let key = self.pending.pop_front()?;
        self.outstanding.insert(key.clone());
        Some((key, self.generation))
    }

    /// Queue one result; false (dropped) when the result predates the
    /// current generation.
    pub fn push_result(&mut self, result: IconResult) -> bool {
        if result.generation != self.generation {
            return false;
        }
        self.results.push_back(result);
        true
    }

    pub fn pop_result(&mut self) -> Option<IconResult> {
        let result = self.results.pop_front()?;
        self.outstanding.remove(&result.key);
        Some(result)
    }

    /// Suppression: drop queued work and stop future requests, and retire
    /// the generation so a late pre-clear result cannot clobber a new
    /// outstanding request. An already-started native lookup may
    /// still finish; its stale result is refused at push, and fresh results
    /// apply to the cache only when the key is still live.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.results.clear();
        self.outstanding.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    /// Drop queued (not yet started) requests for keys no longer drawable.
    /// Returns the number of pruned requests. In-flight lookups finish
    /// normally; their results still pass the generation and liveness gates.
    pub fn prune_pending(&mut self, keep: &HashSet<WindowKey>) -> usize {
        let before = self.pending.len();
        self.pending.retain(|key| keep.contains(key));
        before - self.pending.len()
    }

    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn results_len(&self) -> usize {
        self.results.len()
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.results.is_empty() && self.outstanding.is_empty()
    }
}

/// Mix one miniature's identity plus its icon presence into the repaint
/// hash: full key bytes (HWND reuse changes the hash) plus the icon's
/// precomputed full-content digest (stable FNV over owned bytes, no
/// randomized state). Any artwork byte flips the hash; missing icons hash
/// as identity only. Per-pump cost is one word per miniature.
pub fn mix_mini_icon(mix: &mut dyn FnMut(u64), key: &WindowKey, icon: Option<&IconImage>) {
    mix(key.hwnd);
    mix(u64::from(key.pid));
    for byte in key.creation.bytes() {
        mix(u64::from(byte));
    }
    mix(0xfd);
    if let Some(image) = icon {
        mix(image.digest);
    }
    mix(0xfc);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(hwnd: u64, creation: &str) -> WindowKey {
        WindowKey {
            hwnd,
            pid: 7,
            creation: creation.to_owned(),
        }
    }

    fn mini(w: i32, h: i32) -> Rect {
        Rect { x: 10, y: 20, w, h }
    }

    fn image(fill: u8) -> Arc<IconImage> {
        Arc::new(IconImage::new(32, 32, vec![fill; 32 * 32 * 4]))
    }

    #[test]
    fn icon_centers_and_caps_at_96() {
        // Roomy miniature: 16px artwork centered.
        let rect = icon_rect_for_mini(mini(64, 40), 96).expect("fits");
        assert_eq!((rect.w, rect.h), (16, 16));
        assert_eq!(rect.x, 10 + (64 - 16) / 2);
        assert_eq!(rect.y, 20 + (40 - 16) / 2);
        // Wide-but-short miniature: capped by the short side's usable area.
        let rect = icon_rect_for_mini(mini(64, 16), 96).expect("fits");
        assert_eq!((rect.w, rect.h), (12, 12));
    }

    #[test]
    fn icon_omits_when_not_legible() {
        // Usable area is mini minus 2px per side; 8px minimum needs a 12px mini.
        assert!(icon_rect_for_mini(mini(12, 12), 96).is_some());
        assert!(icon_rect_for_mini(mini(11, 12), 96).is_none());
        assert!(icon_rect_for_mini(mini(12, 11), 96).is_none());
        assert!(icon_rect_for_mini(mini(2, 2), 96).is_none());
    }

    #[test]
    fn icon_scales_with_dpi() {
        // 200%: 32px cap, 16px floor; the 96-DPI roomy mini still fits.
        let rect = icon_rect_for_mini(mini(64, 40), 192).expect("fits");
        assert_eq!((rect.w, rect.h), (32, 32));
        // A mini legible at 96 DPI refuses at 200%.
        assert!(icon_rect_for_mini(mini(12, 12), 192).is_none());
        // Final reported values: cap 16 / floor 8 logical px.
        assert_eq!(crate::pager::dpi_scale(ICON_MAX_96, 96), 16);
        assert_eq!(crate::pager::dpi_scale(ICON_MIN_96, 96), 8);
        assert_eq!(crate::pager::dpi_scale(ICON_MAX_96, 192), 32);
        assert_eq!(crate::pager::dpi_scale(ICON_MIN_96, 192), 16);
    }

    #[test]
    fn cache_requests_misses_retries_and_refreshes() {
        let mut cache = PagerIconCache::new();
        let now = Instant::now();
        let a = key(1, "c1");
        let b = key(2, "c2");
        // Unknown keys request.
        assert_eq!(cache.requests(vec![a.clone(), b.clone()], now).len(), 2);
        // Hits and misses land; fresh entries request nothing.
        cache.insert(a.clone(), Some(image(1)), now);
        cache.insert(b.clone(), None, now);
        assert!(cache.requests(vec![a.clone(), b.clone()], now).is_empty());
        assert_eq!(cache.get(&a), Some(image(1)));
        assert!(cache.get(&b).is_none());
        // Negative entries wait out NEG_RETRY, then re-request.
        assert!(
            cache
                .requests(
                    vec![b.clone()],
                    now + NEG_RETRY - std::time::Duration::from_millis(1),
                )
                .is_empty()
        );
        assert_eq!(
            cache.requests(vec![b.clone()], now + NEG_RETRY),
            vec![b.clone()]
        );
        // Positive entries wait out POS_REFRESH (WM_SETICON pickup).
        assert!(
            cache
                .requests(
                    vec![a.clone()],
                    now + POS_REFRESH - std::time::Duration::from_millis(1),
                )
                .is_empty()
        );
        assert_eq!(
            cache.requests(vec![a.clone()], now + POS_REFRESH),
            vec![a.clone()]
        );
        // No arbitrary cap: every stale key requests (dedup bounds the work).
        let many: Vec<WindowKey> = (0..100).map(|n| key(100 + n, "cap")).collect();
        assert_eq!(cache.requests(many, now).len(), 100);
    }

    #[test]
    fn cache_prune_uses_full_identity_for_hwnd_reuse() {
        let mut cache = PagerIconCache::new();
        let now = Instant::now();
        let old = key(50, "old");
        let fresh = key(50, "new");
        cache.insert(old.clone(), Some(image(3)), now);
        cache.insert(fresh.clone(), Some(image(4)), now);
        // Same HWND, new process creation: only the departed key prunes.
        let live: HashSet<WindowKey> = [fresh.clone()].into_iter().collect();
        assert_eq!(cache.prune(&live), 1);
        assert!(cache.get(&old).is_none());
        assert_eq!(cache.get(&fresh), Some(image(4)));
        // Closed windows prune entirely.
        assert_eq!(cache.prune(&HashSet::new()), 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn channel_dedupes_without_caps_and_clears() {
        let mut channel = IconChannel::new();
        let a = key(1, "c1");
        assert!(channel.push_request(a.clone()));
        assert!(!channel.push_request(a.clone()));
        // No arbitrary cap: every distinct key queues (dedup is the bound).
        for n in 2..200u64 {
            assert!(channel.push_request(key(n, "fill")));
        }
        assert_eq!(channel.pending_len(), 199);
        let (next, generation) = channel.pop_request().expect("queued");
        assert_eq!(next, a);
        assert_eq!(generation, 0);
        // No result cap: every result lands.
        for n in 0..200 {
            assert!(channel.push_result(IconResult {
                key: key(n, "r"),
                image: None,
                source: IconSource::Miss,
                generation,
            }));
        }
        assert_eq!(channel.results_len(), 200);
        channel.clear();
        assert!(channel.is_empty());
    }

    #[test]
    fn channel_tracks_inflight_until_drained() {
        let mut channel = IconChannel::new();
        let a = key(1, "c1");
        assert!(channel.push_request(a.clone()));
        let (next, generation) = channel.pop_request().expect("queued");
        assert_eq!(next, a);
        // In-flight repeats are refused even though nothing is queued.
        assert!(!channel.push_request(a.clone()));
        // The result lands, still outstanding until drained.
        assert!(channel.push_result(IconResult {
            key: a.clone(),
            image: Some(image(7)),
            source: IconSource::Window,
            generation,
        }));
        assert!(!channel.push_request(a.clone()));
        let drained = channel.pop_result().expect("result");
        assert_eq!(drained.key, a);
        // Drained: the key may be requested again (e.g. periodic refresh).
        assert!(channel.push_request(a.clone()));
        assert!(!channel.is_empty());
    }

    #[test]
    fn channel_suppression_generation_drops_late_results() {
        let mut channel = IconChannel::new();
        let a = key(1, "c1");
        let b = key(2, "c2");
        assert!(channel.push_request(a.clone()));
        let (_, old_generation) = channel.pop_request().expect("queued");
        // Suppression clears everything and retires the generation.
        channel.clear();
        assert!(channel.is_empty());
        assert_eq!(channel.generation(), old_generation.wrapping_add(1));
        // The pre-clear lookup finishes late: refused at push.
        assert!(!channel.push_result(IconResult {
            key: a.clone(),
            image: Some(image(8)),
            source: IconSource::Window,
            generation: old_generation,
        }));
        assert_eq!(channel.results_len(), 0);
        // A new request for the same key proceeds under the new generation.
        assert!(channel.push_request(a.clone()));
        let (_, new_generation) = channel.pop_request().expect("queued");
        assert_ne!(new_generation, old_generation);
        assert!(channel.push_result(IconResult {
            key: a.clone(),
            image: None,
            source: IconSource::Miss,
            generation: new_generation,
        }));
        assert_eq!(channel.results_len(), 1);
        // A second clear retires again; unrelated keys are unaffected.
        assert!(channel.push_request(b.clone()));
        channel.clear();
        assert_eq!(channel.pending_len(), 0);
    }

    #[test]
    fn channel_prunes_queued_keys_no_longer_live() {
        let mut channel = IconChannel::new();
        let live_key = key(1, "c1");
        let gone_key = key(2, "c2");
        let flight_key = key(3, "c3");
        assert!(channel.push_request(live_key.clone()));
        assert!(channel.push_request(gone_key.clone()));
        assert!(channel.push_request(flight_key.clone()));
        let (next, generation) = channel.pop_request().expect("live pops first");
        assert_eq!(next, live_key);
        // In-flight work is never cancelled by the prune.
        let keep: HashSet<WindowKey> = [live_key.clone()].into_iter().collect();
        assert_eq!(channel.prune_pending(&keep), 2);
        assert_eq!(channel.pending_len(), 0);
        // The in-flight lookup finishes normally despite the prune.
        assert!(channel.push_result(IconResult {
            key: live_key.clone(),
            image: None,
            source: IconSource::Miss,
            generation,
        }));
        assert_eq!(channel.pop_result().expect("result").key, live_key);
        // A still-live key may be requested again afterwards.
        assert!(channel.push_request(live_key.clone()));
        assert_eq!(channel.prune_pending(&keep), 0);
        assert_eq!(channel.pending_len(), 1);
    }

    #[test]
    fn mix_hashes_every_artwork_byte() {
        let words = |icon: &IconImage| {
            let mut out = Vec::new();
            mix_mini_icon(&mut |w| out.push(w), &key(9, "same"), Some(icon));
            out
        };
        let base = IconImage::new(32, 32, vec![5; 32 * 32 * 4]);
        // A single byte flip at an index the old 64-sampled scheme skipped
        // (63, 65, 127) changes the repaint hash.
        for index in [63usize, 65, 127, 4095] {
            let mut bgra = vec![5; 32 * 32 * 4];
            bgra[index] = 6;
            let flipped = IconImage::new(32, 32, bgra);
            assert_ne!(
                words(&base),
                words(&flipped),
                "byte {index} must flip the hash"
            );
        }
        // Same bytes hash stably; digest is content-derived.
        assert_eq!(
            words(&base),
            words(&IconImage::new(32, 32, vec![5; 32 * 32 * 4]))
        );
        assert_eq!(
            base.digest,
            IconImage::new(32, 32, vec![5; 32 * 32 * 4]).digest
        );
    }
    #[test]
    fn mix_covers_identity_and_artwork_stably() {
        let words = |k: &WindowKey, icon: Option<&IconImage>| {
            let mut out = Vec::new();
            mix_mini_icon(&mut |w| out.push(w), k, icon);
            out
        };
        let a = key(9, "same");
        let again = words(&a, Some(&image(5)));
        assert_eq!(again, words(&a, Some(&image(5))));
        // HWND reuse (new creation) hashes differently.
        assert_ne!(again, words(&key(9, "other"), Some(&image(5))));
        // Artwork change hashes differently; missing icon differs from hit.
        assert_ne!(again, words(&a, Some(&image(6))));
        assert_ne!(again, words(&a, None));
    }

    #[test]
    fn image_validates_shape() {
        assert!(image(0).is_valid());
        assert!(!IconImage::new(32, 32, vec![0; 10]).is_valid());
    }
}
