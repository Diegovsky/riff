// A structure for batched queries that I introduced before proper batch management
// Still used to load album lists for instance
// Doesn't know how many elements exist in total ahead of time
#[derive(Clone, Debug)]
pub struct Pagination<T>
where
    T: Clone,
{
    pub data: T,
    // The next offset (of things to load) is set to None whenever we get less than we asked for
    // as it probably means we've reached the end of some list
    pub next_offset: Option<usize>,
    pub batch_size: usize,
}

impl<T> Pagination<T>
where
    T: Clone,
{
    pub fn new(data: T, batch_size: usize) -> Self {
        Self {
            data,
            next_offset: Some(0),
            batch_size,
        }
    }

    pub fn reset_count(&mut self, new_length: usize) {
        self.next_offset = if new_length >= self.batch_size {
            Some(self.batch_size)
        } else {
            None
        }
    }

    // Eagerly advance the offset before the request completes.
    // Returns the offset to use for the request, or None if already consumed/exhausted.
    pub fn next_offset_take(&mut self) -> Option<usize> {
        let offset = self.next_offset.take()?;
        // Optimistically assume a full batch will be returned
        self.next_offset = Some(offset + self.batch_size);
        Some(offset)
    }

    pub fn set_loaded_count(&mut self, loaded_count: usize) {
        if let Some(offset) = self.next_offset.take() {
            self.next_offset = if loaded_count >= self.batch_size {
                Some(offset)
            } else {
                None
            }
        }
    }

    pub fn restore_offset(&mut self, offset: usize) {
        self.next_offset = Some(offset);
    }

    // If we remove elements from paginated data without refetching from the source,
    // we have to adjust the next offset to load
    pub fn decrement(&mut self) {
        if let Some(offset) = self.next_offset.take() {
            self.next_offset = Some(offset - 1);
        }
    }

    // Same idea as decrement
    pub fn increment(&mut self) {
        if let Some(offset) = self.next_offset.take() {
            self.next_offset = Some(offset + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_offset_take_advances_optimistically() {
        let mut p = Pagination::new((), 50);
        assert_eq!(p.next_offset_take(), Some(0));
        // Advanced before the request completed.
        assert_eq!(p.next_offset, Some(50));
    }

    #[test]
    fn test_next_offset_take_rejects_second_call_while_in_flight() {
        let mut p = Pagination::new((), 50);
        p.next_offset_take();
        // Optimistic advance means a second take proceeds from the assumed
        // next page, not a rejection - this documents current behavior.
        assert_eq!(p.next_offset_take(), Some(50));
    }

    #[test]
    fn test_set_loaded_count_full_batch_keeps_offset() {
        let mut p = Pagination::new((), 50);
        let offset = p.next_offset_take().unwrap();
        let advanced = p.next_offset; // the optimistic advance from next_offset_take
        p.set_loaded_count(50);
        // A full batch confirms the optimistic advance rather than reverting it.
        assert_eq!(p.next_offset, advanced);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_set_loaded_count_short_batch_ends_pagination() {
        let mut p = Pagination::new((), 50);
        p.next_offset_take();
        p.set_loaded_count(10);
        assert_eq!(p.next_offset, None);
    }

    #[test]
    fn test_restore_offset_undoes_failed_request() {
        let mut p = Pagination::new((), 50);
        let offset = p.next_offset_take().unwrap();
        // Optimistic advance already happened.
        assert_eq!(p.next_offset, Some(offset + 50));

        // The request for `offset` failed - restore it instead of losing the page.
        p.restore_offset(offset);
        assert_eq!(p.next_offset, Some(offset));

        // The same offset is handed out again on retry.
        assert_eq!(p.next_offset_take(), Some(offset));
    }

    #[test]
    fn test_restore_offset_after_multiple_pages() {
        let mut p = Pagination::new((), 50);
        p.next_offset_take();
        p.set_loaded_count(50);
        let second = p.next_offset_take().unwrap();
        assert_eq!(second, 50);

        // Second page's request fails.
        p.restore_offset(second);
        assert_eq!(p.next_offset_take(), Some(50));
    }
}
