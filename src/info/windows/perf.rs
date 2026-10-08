//! Dedicated video memory in use, read from the GPU performance counters through PDH.

use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_LARGE, PDH_HQUERY, PDH_MORE_DATA,
};

use super::to_wide;

const DEDICATED_USAGE: &str = r"\GPU Adapter Memory(*)\Dedicated Usage";

/// Dedicated video memory in use, in bytes: the largest value over the GPU adapters.
/// `None` when the counters cannot be read.
pub(super) fn dedicated_vram_used() -> Option<u64> {
    let query = Query::open()?;
    let path = to_wide(DEDICATED_USAGE);
    let mut counter = null_mut();
    // SAFETY: the query is open, `path` is NUL-terminated and `counter` receives the handle.
    let status = unsafe { PdhAddEnglishCounterW(query.0, path.as_ptr(), 0, &mut counter) };
    if status != ERROR_SUCCESS {
        return None;
    }
    // SAFETY: the query is open and holds the counter added above.
    if unsafe { PdhCollectQueryData(query.0) } != ERROR_SUCCESS {
        return None;
    }
    let mut size = 0_u32;
    let mut count = 0_u32;
    // SAFETY: a null buffer only asks for the size PDH needs, which `size` receives.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_LARGE, &mut size, &mut count, null_mut())
    };
    if status != PDH_MORE_DATA {
        return None;
    }
    let capacity = usize::try_from(size)
        .ok()?
        .div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
    let mut items = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); capacity];
    // SAFETY: `items` holds at least `size` bytes, the buffer size PDH asked for.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_LARGE,
            &mut size,
            &mut count,
            items.as_mut_ptr(),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let items = items.get(..usize::try_from(count).ok()?)?;
    largest_valid(items.iter().map(|item| {
        // SAFETY: the items were requested as PDH_FMT_LARGE, so PDH wrote `largeValue`.
        let value = unsafe { item.FmtValue.Anonymous.largeValue };
        (item.FmtValue.CStatus, value)
    }))
}

/// The largest value among the items whose data is valid. Pure, so the choice is tested
/// without PDH.
fn largest_valid(items: impl IntoIterator<Item = (u32, i64)>) -> Option<u64> {
    items
        .into_iter()
        .filter(|(status, _)| matches!(*status, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA))
        .filter_map(|(_, value)| u64::try_from(value).ok())
        .max()
}

/// A PDH query, closed when dropped.
struct Query(PDH_HQUERY);

impl Query {
    fn open() -> Option<Query> {
        let mut query = null_mut();
        // SAFETY: a null data source reads the live counters, and `query` receives the handle.
        let status = unsafe { PdhOpenQueryW(null(), 0, &mut query) };
        (status == ERROR_SUCCESS).then(|| Query(query))
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful PdhOpenQueryW and is closed only here.
        unsafe { PdhCloseQuery(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::System::Performance::PDH_CSTATUS_INVALID_DATA;

    use super::{dedicated_vram_used, largest_valid};

    #[test]
    fn largest_valid_value_wins() {
        assert_eq!(largest_valid([(0, 5), (1, 9), (0, 7)]), Some(9));
    }

    #[test]
    fn invalid_statuses_and_negative_values_are_skipped() {
        assert_eq!(
            largest_valid([(PDH_CSTATUS_INVALID_DATA, 99), (0, -4), (1, 3)]),
            Some(3)
        );
        assert_eq!(largest_valid([(PDH_CSTATUS_INVALID_DATA, 1)]), None);
    }

    #[test]
    fn no_valid_items_gives_none() {
        assert_eq!(largest_valid(Vec::new()), None);
    }

    #[test]
    fn reading_the_counters_does_not_panic() {
        let _ = dedicated_vram_used();
    }
}
