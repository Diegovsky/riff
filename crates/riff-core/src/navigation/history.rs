#[derive(Debug, Clone)]
pub struct History<E> {
    entries: Vec<E>,
    cursor: usize,
    capacity: usize,
}

impl<E> History<E> {
    pub fn new(initial: E, capacity: usize) -> Self {
        Self {
            entries: vec![initial],
            cursor: 0,
            capacity: capacity.max(1),
        }
    }

    pub fn current(&self) -> &E {
        &self.entries[self.cursor]
    }

    pub fn current_mut(&mut self) -> &mut E {
        &mut self.entries[self.cursor]
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn entries(&self) -> &[E] {
        &self.entries
    }

    pub fn entries_mut(&mut self) -> &mut [E] {
        &mut self.entries
    }

    pub fn can_go_back(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    pub fn push(&mut self, entry: E) {
        self.entries.truncate(self.cursor + 1);
        self.entries.push(entry);
        let excess = self.entries.len().saturating_sub(self.capacity);
        self.entries.drain(..excess);
        self.cursor = self.entries.len() - 1;
    }

    pub fn back(&mut self) -> Option<&E> {
        self.go_to(self.cursor.checked_sub(1)?)
    }

    pub fn forward(&mut self) -> Option<&E> {
        self.go_to(self.cursor + 1)
    }

    pub fn go_to(&mut self, index: usize) -> Option<&E> {
        if index >= self.entries.len() {
            return None;
        }
        self.cursor = index;
        Some(self.current())
    }

    pub fn remove_current_if(&mut self, pred: impl FnOnce(&E) -> bool) -> Option<E> {
        if !self.can_go_back() || !pred(self.current()) {
            return None;
        }
        let removed = self.entries.remove(self.cursor);
        self.cursor -= 1;
        Some(removed)
    }

    pub fn back_entries(&self) -> impl Iterator<Item = (usize, &E)> {
        self.entries[..self.cursor].iter().enumerate().rev()
    }

    pub fn forward_entries(&self) -> impl Iterator<Item = (usize, &E)> {
        let start = self.cursor + 1;
        self.entries[start..]
            .iter()
            .enumerate()
            .map(move |(i, e)| (start + i, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(items: &[u32]) -> History<u32> {
        let mut history = History::new(items[0], 10);
        for item in &items[1..] {
            history.push(*item);
        }
        history
    }

    #[test]
    fn back_and_forward_move_the_cursor() {
        let mut h = history(&[1, 2, 3]);
        assert_eq!(h.back(), Some(&2));
        assert_eq!(h.back(), Some(&1));
        assert_eq!(h.back(), None);
        assert_eq!(*h.current(), 1);
        assert_eq!(h.forward(), Some(&2));
        assert_eq!(h.forward(), Some(&3));
        assert_eq!(h.forward(), None);
    }

    #[test]
    fn pushing_drops_forward_entries() {
        let mut h = history(&[1, 2, 3]);
        h.back();
        h.back();
        h.push(4);
        assert_eq!(h.entries(), &[1, 4]);
        assert!(!h.can_go_forward());
    }

    #[test]
    fn capacity_drops_the_oldest() {
        let mut h = History::new(0, 3);
        for i in 1..=5 {
            h.push(i);
        }
        assert_eq!(h.entries(), &[3, 4, 5]);
        assert_eq!(h.cursor(), 2);
    }

    #[test]
    fn go_to_rejects_out_of_range() {
        let mut h = history(&[1, 2]);
        assert_eq!(h.go_to(5), None);
        assert_eq!(*h.current(), 2);
        assert_eq!(h.go_to(0), Some(&1));
    }

    #[test]
    fn remove_current_steps_back() {
        let mut h = history(&[1, 2, 3]);
        h.back();
        assert_eq!(h.remove_current_if(|e| *e == 2), Some(2));
        assert_eq!(h.entries(), &[1, 3]);
        assert_eq!(*h.current(), 1);
    }

    #[test]
    fn remove_current_keeps_the_first_entry() {
        let mut h = history(&[1]);
        assert_eq!(h.remove_current_if(|_| true), None);
        let mut h = history(&[1, 2]);
        assert_eq!(h.remove_current_if(|e| *e == 1), None);
        assert_eq!(h.entries(), &[1, 2]);
    }

    #[test]
    fn neighbours_are_listed_nearest_first() {
        let mut h = history(&[1, 2, 3, 4]);
        h.go_to(1);
        let back: Vec<_> = h.back_entries().collect();
        assert_eq!(back, vec![(0, &1)]);
        let forward: Vec<_> = h.forward_entries().collect();
        assert_eq!(forward, vec![(2, &3), (3, &4)]);
    }
}
