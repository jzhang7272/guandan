//! Rule violations (TECH_SPEC.md §3.13).

use serde::{Deserialize, Serialize};

/// Why a game action was refused. Fieldless on purpose: it serializes as a
/// bare string code on the wire (e.g. `"NotYourTurn"`), and the `Display`
/// text becomes `Rejected.message` for the player to read.
///
/// Ambiguity is deliberately *not* here: a play with several readings is an
/// `Ok(NeedsDeclaration)` outcome, since the player did nothing wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
pub enum ActionError {
    #[error("That action isn't allowed right now")]
    WrongPhase,
    #[error("It is not your turn")]
    NotYourTurn,
    #[error("Those cards are not in your hand")]
    CardsNotInHand,
    #[error("Those cards don't form a valid combination")]
    NotAValidCombo,
    #[error("That play doesn't beat the current play")]
    DoesNotBeatCurrent,
    #[error("Those cards can't be played as the declared combination")]
    InvalidDeclaration,
    #[error("You can't pass when you are leading")]
    CannotPassWhenLeading,
    /// TakeBack with no play of this seat's to undo: nobody has played, the
    /// last play was someone else's, someone has acted since, or it ended
    /// the deal (GAME_RULES.md house rule #10).
    #[error("There's nothing of yours to take back")]
    NothingToTakeBack,
    #[error("You don't owe a tribute")]
    NotATributePayer,
    #[error("You have already paid your tribute")]
    AlreadyPaid,
    #[error("That card can't be paid as tribute")]
    InvalidTributeCard,
    #[error("You aren't receiving a tribute")]
    NotATributeReceiver,
    #[error("Wait until every tribute has been paid")]
    TributeNotComplete,
    #[error("You have already returned a card")]
    AlreadyReturned,
    #[error("That card can't be returned")]
    InvalidReturnCard,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_your_turn_message_matches_the_wire_example() {
        // TECH_SPEC.md §4 "Rejected" example.
        assert_eq!(ActionError::NotYourTurn.to_string(), "It is not your turn");
    }

    #[test]
    fn serializes_as_a_bare_string() {
        assert_eq!(
            serde_json::to_string(&ActionError::CardsNotInHand).unwrap(),
            r#""CardsNotInHand""#
        );
        let back: ActionError = serde_json::from_str(r#""AlreadyReturned""#).unwrap();
        assert_eq!(back, ActionError::AlreadyReturned);
    }

    #[test]
    fn every_variant_has_a_non_empty_message() {
        let all = [
            ActionError::WrongPhase,
            ActionError::NotYourTurn,
            ActionError::CardsNotInHand,
            ActionError::NotAValidCombo,
            ActionError::DoesNotBeatCurrent,
            ActionError::InvalidDeclaration,
            ActionError::CannotPassWhenLeading,
            ActionError::NothingToTakeBack,
            ActionError::NotATributePayer,
            ActionError::AlreadyPaid,
            ActionError::InvalidTributeCard,
            ActionError::NotATributeReceiver,
            ActionError::TributeNotComplete,
            ActionError::AlreadyReturned,
            ActionError::InvalidReturnCard,
        ];
        for error in all {
            assert!(!error.to_string().is_empty(), "{error:?}");
        }
    }
}
