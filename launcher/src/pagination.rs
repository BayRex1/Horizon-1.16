//! Paging shared by remote content searches and local instance collections.
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageSize {
    Ten,
    #[default]
    Twenty,
    Fifty,
    Hundred,
}

impl PageSize {
    pub const ALL: [Self; 4] = [Self::Ten, Self::Twenty, Self::Fifty, Self::Hundred];

    pub const fn count(self) -> usize {
        match self {
            Self::Ten => 10,
            Self::Twenty => 20,
            Self::Fifty => 50,
            Self::Hundred => 100,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Pagination {
    pub index: usize,
    pub size: PageSize,
}

impl Pagination {
    pub fn offset(self) -> usize {
        self.index.saturating_mul(self.size.count())
    }

    pub fn pages(self, total: usize) -> usize {
        total.div_ceil(self.size.count()).max(1)
    }

    pub fn clamp(&mut self, total: usize) {
        self.index = self.index.min(self.pages(total) - 1);
    }

    pub fn range(self, total: usize) -> Range<usize> {
        let start = self.offset().min(total);
        start..start.saturating_add(self.size.count()).min(total)
    }

    pub fn set_size(&mut self, size: PageSize) {
        self.size = size;
        self.index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_cover_each_item_once_including_a_partial_final_page() {
        for size in PageSize::ALL {
            let mut paging = Pagination {
                size,
                ..Default::default()
            };
            let mut seen = vec![];
            for index in 0..paging.pages(347) {
                paging.index = index;
                seen.extend(paging.range(347));
            }
            assert_eq!(seen, (0..347).collect::<Vec<_>>());
        }
    }

    #[test]
    fn filtering_and_page_size_changes_cannot_leave_an_invalid_page() {
        let mut paging = Pagination {
            index: 16,
            ..Default::default()
        };
        paging.clamp(0);
        assert_eq!(paging.index, 0);
        assert_eq!(paging.range(0), 0..0);
        paging.index = 5;
        paging.clamp(21);
        assert_eq!(paging.index, 1);
        assert_eq!(paging.range(21), 20..21);
        paging.set_size(PageSize::Hundred);
        assert_eq!(paging.index, 0);
        assert_eq!(paging.range(21), 0..21);
    }
}
