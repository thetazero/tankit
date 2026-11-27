//! How smart realms and companies run their war.
//!
//! They play by the same rules as the others, knowing no more than they do:
//! only what they decide differs.
//!
//! **Pilots.** Fights go to whoever fills more seats, and crystal in a vault
//! only waits for someone to take it. So a smart realm budgets: it keeps as
//! many pilots as its income and a share of its savings pay for, whatever
//! its last fights showed, and lets them go only when the budget's short.

use super::Rules;
use super::realm::Realm;

/// Share of its income a smart realm's payroll may take for good...
const PAYROLL: f32 = 0.6;
/// ... and the ticks it spreads its savings over, to pay for more.
const RUNWAY: f32 = 150.0;

impl Realm {
    /// The pilots a smart realm can keep paying.
    pub(crate) fn crew_budget(&self, rules: &Rules) -> u32 {
        let net = self.income.max(0.0) * (1.0 - rules.research);
        ((PAYROLL * net + self.crystals.max(0.0) / RUNWAY) / self.pay(rules)) as u32
    }

    /// A smart realm takes pilots on toward its budget, those out of work
    /// first, or lets some go. Returns those let go.
    pub(crate) fn crew_up(&mut self, rules: &Rules, free: u32) -> u32 {
        let budget = self.crew_budget(rules);
        let open = budget.saturating_sub(self.pilots);
        if open > 0 {
            let n = rules.train.min(open);
            let untrained = n.saturating_sub(free);
            let fee = rules.train_cost.max(1e-6);
            let trained = untrained.min((self.crystals / fee) as u32);
            // (one tick's intake is all of one or all of the other, as anyone's)
            if free > 0 {
                let n = n.min(free);
                self.pilots += n;
                self.hired = n;
            } else {
                self.pilots += trained;
                self.crystals -= trained as f32 * rules.train_cost;
            }
            0
        } else {
            let n = rules.train.min((self.pilots - budget).saturating_sub(budget / 10));
            self.pilots -= n;
            n
        }
    }
}
