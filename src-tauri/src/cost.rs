//! Token + chi phí của 1 lượt đọc. Chi phí tính lúc ĐỌC từ `calls` × bảng giá HIỆN TẠI →
//! người dùng sửa giá là thấy ngay. Model chưa có giá → `null`.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use crate::config::Pricing;
use crate::models::CallUsage;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReaderUsage {
    pub agent: String,
    pub prompt: u64,
    pub cached: u64,
    pub completion: u64,
    pub calls: u32,
    pub estimated: bool,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct UsageTotals {
    pub prompt: u64,
    pub cached: u64,
    pub completion: u64,
    pub estimated: bool,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct UsageBreakdown {
    pub by_reader: Vec<ReaderUsage>,
    pub totals: UsageTotals,
    /// Model đã dùng nhưng chưa có trong bảng giá (sắp xếp, không trùng).
    pub unpriced_models: Vec<String>,
}

/// Cộng dồn 1 nhóm lần gọi; chi phí None ngay khi có 1 lần gọi không có giá.
#[derive(Default)]
struct Acc {
    prompt: u64,
    cached: u64,
    completion: u64,
    calls: u32,
    estimated: bool,
    cost: Option<f64>,
}

impl Acc {
    fn new() -> Self {
        Self { cost: Some(0.0), ..Self::default() }
    }

    fn add(&mut self, c: &CallUsage, price: Option<f64>) {
        self.prompt += c.prompt;
        self.cached += c.cached;
        self.completion += c.completion;
        self.calls += 1;
        self.estimated |= c.estimated;
        self.cost = self.cost.zip(price).map(|(a, b)| a + b);
    }
}

pub fn usage_breakdown(calls: &[CallUsage], pricing: &Pricing) -> UsageBreakdown {
    let mut unpriced = BTreeSet::new();
    let mut all = Acc::new();
    // Giữ thứ tự xuất hiện (lần gọi được ghi theo thời gian).
    let mut readers: Vec<(String, Acc)> = Vec::new();
    let mut ix: HashMap<String, usize> = HashMap::new();
    for c in calls {
        let price = pricing.get(&c.model).map(|p| p.cost(c.prompt, c.cached, c.completion));
        if price.is_none() {
            unpriced.insert(c.model.clone());
        }
        all.add(c, price);
        let i = *ix.entry(c.agent.clone()).or_insert_with(|| {
            readers.push((c.agent.clone(), Acc::new()));
            readers.len() - 1
        });
        readers[i].1.add(c, price);
    }
    UsageBreakdown {
        by_reader: readers
            .into_iter()
            .map(|(agent, a)| ReaderUsage {
                agent,
                prompt: a.prompt,
                cached: a.cached,
                completion: a.completion,
                calls: a.calls,
                estimated: a.estimated,
                cost_usd: a.cost,
            })
            .collect(),
        totals: UsageTotals {
            prompt: all.prompt,
            cached: all.cached,
            completion: all.completion,
            estimated: all.estimated,
            cost_usd: all.cost,
        },
        unpriced_models: unpriced.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelPrice;

    fn call(id: u32, agent: &str, model: &str, p: u64, cached: u64, c: u64) -> CallUsage {
        CallUsage {
            call_id: id,
            agent: agent.into(),
            kind: "reader".into(),
            model: model.into(),
            prompt: p,
            cached,
            completion: c,
            estimated: false,
            finish: "stop".into(),
            latency_ms: 5,
        }
    }

    fn pricing() -> Pricing {
        let mut p = Pricing::new();
        p.insert("cheap".into(), ModelPrice { input_per_m: 1.0, cached_per_m: 0.5, output_per_m: 2.0 });
        p
    }

    #[test]
    fn cost_is_computed_per_reader_with_current_pricing() {
        let calls = vec![
            call(0, "classic", "cheap", 1_000_000, 0, 500_000),
            call(1, "jungian", "cheap", 1_000_000, 1_000_000, 0),
            call(2, "classic", "cheap", 0, 0, 1_000_000),
        ];
        let u = usage_breakdown(&calls, &pricing());
        assert_eq!(u.totals.cost_usd, Some(2.0 + 0.5 + 2.0));
        assert_eq!((u.totals.prompt, u.totals.cached, u.totals.completion), (2_000_000, 1_000_000, 1_500_000));
        assert_eq!(
            u.by_reader.iter().map(|r| (r.agent.as_str(), r.cost_usd)).collect::<Vec<_>>(),
            vec![("classic", Some(4.0)), ("jungian", Some(0.5))]
        );
        assert!(u.unpriced_models.is_empty());
    }

    #[test]
    fn missing_price_makes_cost_null() {
        let calls = vec![call(0, "classic", "cheap", 10, 0, 10), call(1, "jungian", "mystery", 10, 0, 10)];
        let u = usage_breakdown(&calls, &pricing());
        assert_eq!(u.totals.cost_usd, None);
        assert_eq!(u.unpriced_models, vec!["mystery"]);
        assert!(u.by_reader[0].cost_usd.is_some() && u.by_reader[1].cost_usd.is_none());
        assert_eq!(usage_breakdown(&calls, &Pricing::new()).totals.cost_usd, None);
    }
}
