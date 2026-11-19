//! 계측 로그가 함께 쓰는 고정 크기 표본 창과 백분위 계산.
//! 렌더 프레임 집계(`gfx::perf`)와 IPC 회차 집계가 같은 계산으로 p50/p99/max를 낸다.

use std::collections::VecDeque;

/// 최근 `capacity`개 표본만 보관하는 창. 가득 차면 가장 오래된 표본을 버린다.
pub(crate) struct SampleWindow<T> {
    samples: VecDeque<T>,
    capacity: usize,
}

impl<T> SampleWindow<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
        }
    }

    pub(crate) fn push(&mut self, sample: T) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub(crate) fn len(&self) -> usize {
        self.samples.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub(crate) fn last(&self) -> Option<&T> {
        self.samples.back()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.samples.iter()
    }

    /// 표본에서 고른 값의 (p50, p99, max). 창이 비었으면 모두 0이다.
    pub(crate) fn percentiles(&self, select: impl Fn(&T) -> f64) -> Percentiles {
        let mut values: Vec<f64> = self.samples.iter().map(select).collect();
        Percentiles::of(&mut values)
    }
}

/// 한 값의 p50·p99·max.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Percentiles {
    pub p50: f64,
    pub p99: f64,
    pub max: f64,
}

impl Percentiles {
    /// 값을 정렬해 nearest-rank 방식으로 고른다. 비었으면 모두 0이다.
    pub(crate) fn of(values: &mut [f64]) -> Self {
        let n = values.len();
        if n == 0 {
            return Self::default();
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Self {
            p50: values[n / 2],
            p99: values[((n * 99) / 100).min(n - 1)],
            max: values[n - 1],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_of_sorted_window() {
        let mut window = SampleWindow::new(100);
        for i in (1..=100).rev() {
            window.push(i as f64);
        }
        let p = window.percentiles(|v| *v);
        assert_eq!(p.p50, 51.0);
        assert_eq!(p.p99, 100.0);
        assert_eq!(p.max, 100.0);
    }

    #[test]
    fn window_keeps_only_capacity_samples() {
        let mut window = SampleWindow::new(3);
        for i in 0..10 {
            window.push(i);
        }
        assert_eq!(window.len(), 3);
        assert_eq!(window.iter().copied().collect::<Vec<_>>(), vec![7, 8, 9]);
    }

    #[test]
    fn empty_window_reports_zero() {
        let window: SampleWindow<f64> = SampleWindow::new(4);
        assert_eq!(window.percentiles(|v| *v), Percentiles::default());
    }
}
