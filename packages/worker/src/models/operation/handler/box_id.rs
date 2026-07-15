use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{info, warn};

/// Number of disjoint slots the host's 0..1000 isolate box-id space is split
/// into — i.e. the maximum number of worker processes that can share a host
/// without box-id collisions. Each worker claims exactly one slot for life.
const BOX_ID_SLOTS: u32 = 16;
/// Box ids available to a single worker (slot width).
const BOX_IDS_PER_SLOT: u32 = 1000 / BOX_ID_SLOTS;

/// Within-slot box-id counter (also reused as a generic process-local
/// uniqueness counter for e.g. channel directory names).
pub(super) static NEXT_BOX_ID: AtomicU32 = AtomicU32::new(0);

/// First box id of this worker process's slot, lazily claimed on first use.
///
/// isolate box ids are a host-wide 0..1000 namespace, and `isolate --init` on
/// an idle, already-initialized box silently *re-initializes* it (verified:
/// exit 0). So two worker processes that pick the same box id clobber each
/// other's sandbox during setup — the corruption only surfaces later as
/// "box currently in use by another process" → a spurious SystemError. Pure
/// retry can't fix the clobber-during-setup window (there is no error to retry
/// on). Instead each worker claims a disjoint slot via an advisory file lock
/// (released automatically by the kernel when the process dies) and only ever
/// allocates box ids inside `[base, base + BOX_IDS_PER_SLOT)`, making
/// cross-worker collisions impossible for up to `BOX_ID_SLOTS` workers/host.
static BOX_SLOT_BASE: std::sync::LazyLock<u32> =
    std::sync::LazyLock::new(|| claim_box_id_slot() * BOX_IDS_PER_SLOT);

/// Claim the lowest free box-id slot for the lifetime of this process using a
/// non-blocking `flock` on a per-slot lock file. The locked fd is intentionally
/// leaked so the lock is held until the process exits, at which point the kernel
/// releases it and the slot becomes available to a restarted worker.
fn claim_box_id_slot() -> u32 {
    use std::os::unix::io::IntoRawFd;
    for slot in 0..BOX_ID_SLOTS {
        let path = std::env::temp_dir().join(format!("broccoli-box-slot-{slot}.lock"));
        let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&path)
        else {
            continue;
        };
        let fd = file.into_raw_fd();
        // SAFETY: `fd` is a freshly opened, owned file descriptor.
        if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            // Intentionally leak `fd`: the advisory lock must be held for the
            // whole process lifetime, so the descriptor must never be closed.
            info!(
                slot,
                base = slot * BOX_IDS_PER_SLOT,
                "Claimed isolate box-id slot"
            );
            return slot;
        }
        // Slot owned by another process — close and try the next one.
        // SAFETY: `fd` is owned here and not used after this close.
        unsafe { libc::close(fd) };
    }
    warn!(
        slots = BOX_ID_SLOTS,
        "No free isolate box-id slot (more workers than slots on this host); \
         falling back to slot 0 — box-id collisions are possible"
    );
    0
}

pub(super) fn allocate_box_id() -> String {
    let offset = NEXT_BOX_ID.fetch_add(1, Ordering::Relaxed) % BOX_IDS_PER_SLOT;
    (*BOX_SLOT_BASE + offset).to_string()
}
