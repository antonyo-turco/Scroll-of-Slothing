//! Dice roller service. Spec: docs/features/dice-roller.md.
//! One stateless `dice_roll`: parse standard notation and roll it. The recent-roll
//! list is session-only UI state (`app/src/lib/dice.svelte.ts`), so nothing is stored.

pub use sloth_core::dice::{DieGroup, Roll as RollResult};
use sloth_core::dice::{Expression, roll_expr};

use crate::{Result, Service, ServiceError};

impl Service {
    /// Rolls a dice expression like `2d6+3`, `d20`, `2d8+1d6+4`, `d%`, `adv+5` or `dis`.
    pub async fn dice_roll(&self, notation: &str) -> Result<RollResult> {
        self.dice_roll_seeded(notation, fastrand::u64(..))
    }

    pub(crate) fn dice_roll_seeded(&self, notation: &str, seed: u64) -> Result<RollResult> {
        let expr =
            Expression::parse(notation).map_err(|e| ServiceError::InvalidInput(e.to_string()))?;
        Ok(roll_expr(&expr, seed))
    }
}

#[cfg(test)]
mod tests {
    use crate::temp_service;

    #[tokio::test(flavor = "multi_thread")]
    async fn rolls_are_deterministic_for_a_seed() {
        let (_d, svc) = temp_service();
        let a = svc.dice_roll_seeded("2d6+3", 42).unwrap();
        assert_eq!(a, svc.dice_roll_seeded("2d6+3", 42).unwrap());
        assert_eq!((a.groups.len(), a.groups[0].sides, a.groups[0].faces.len()), (1, 6, 2));
        let faces: i64 = a.groups[0].faces.iter().map(|&f| i64::from(f)).sum();
        assert_eq!(a.total, faces + 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn percentile_multi_term_and_advantage() {
        let (_d, svc) = temp_service();
        let p = svc.dice_roll("d%").await.unwrap();
        assert_eq!((p.expression.as_str(), p.groups[0].sides), ("d100", 100));
        let m = svc.dice_roll_seeded("2d8+1d6-1", 7).unwrap();
        let sum = |i: usize| m.groups[i].faces.iter().map(|&f| i64::from(f)).sum::<i64>();
        assert_eq!(m.total, sum(0) + sum(1) - 1);
        let a = svc.dice_roll("adv+3").await.unwrap();
        assert_eq!(a.total, i64::from(a.groups[0].kept.unwrap()) + 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bad_input_is_rejected() {
        let (_d, svc) = temp_service();
        for bad in ["", "abc", "300d6", "2d0"] {
            assert!(svc.dice_roll(bad).await.is_err(), "{bad}");
        }
    }
}
