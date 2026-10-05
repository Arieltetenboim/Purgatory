//! Production death notice and respawn confirm (authoritative Health only).

use purgatory_protocol::ReplicatedHealth;

use crate::ui_dialog::{
    DialogAction, DialogButton, MessageDialog, MessageDialogRequest, MessageDialogResult,
};

/// Stable dialog id so modal results are not confused with drop confirmation.
pub(crate) const DEATH_RESPAWN_DIALOG_ID: u64 = 0xDEAD_0001;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeathRespawnConfirm {
    /// User confirmed respawn; caller should send once if not already pending.
    SendRespawn,
    /// Ignored click while respawn is already pending.
    IgnoredPending,
    /// Not a death-dialog result.
    NotDeathDialog,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DeathRespawnUi {
    respawn_pending: bool,
    /// Last pending flag used when the death dialog was opened (for refresh).
    dialog_pending_snapshot: bool,
    tracking: bool,
}

impl DeathRespawnUi {
    pub(crate) fn respawn_pending(&self) -> bool {
        self.respawn_pending
    }

    pub(crate) fn clear(&mut self) {
        self.respawn_pending = false;
        self.dialog_pending_snapshot = false;
        self.tracking = false;
    }

    pub(crate) fn note_respawn_send_failed(&mut self) {
        self.respawn_pending = false;
        self.dialog_pending_snapshot = false;
    }

    pub(crate) fn note_respawn_sent(&mut self) {
        self.respawn_pending = true;
    }

    pub(crate) fn is_death_dialog_active(dialog: &MessageDialog) -> bool {
        dialog.active_id() == Some(DEATH_RESPAWN_DIALOG_ID)
    }

    /// Keep the production death modal aligned with replicated local Health.
    pub(crate) fn sync_modal(&mut self, dialog: &mut MessageDialog, dead: bool) {
        if !dead {
            self.clear();
            if DeathRespawnUi::is_death_dialog_active(dialog) {
                dialog.close();
            }
            return;
        }

        let needs_open = !DeathRespawnUi::is_death_dialog_active(dialog)
            || self.dialog_pending_snapshot != self.respawn_pending;

        if !needs_open {
            self.tracking = true;
            return;
        }

        if dialog.is_active() && DeathRespawnUi::is_death_dialog_active(dialog) {
            dialog.close();
        } else if dialog.is_active() {
            // Death takes priority over other modals while the player is dead.
            dialog.close();
        }

        if dialog.open(death_dialog_request(self.respawn_pending)) {
            self.tracking = true;
            self.dialog_pending_snapshot = self.respawn_pending;
        }
    }

    pub(crate) fn interpret_result(
        result: MessageDialogResult,
        respawn_pending: bool,
    ) -> DeathRespawnConfirm {
        if result.id != DEATH_RESPAWN_DIALOG_ID {
            return DeathRespawnConfirm::NotDeathDialog;
        }
        match result.action {
            DialogAction::Ok | DialogAction::Confirm if respawn_pending => {
                DeathRespawnConfirm::IgnoredPending
            }
            DialogAction::Ok | DialogAction::Confirm => DeathRespawnConfirm::SendRespawn,
            _ => DeathRespawnConfirm::IgnoredPending,
        }
    }
}

#[must_use]
pub(crate) fn authoritative_dead(health: Option<ReplicatedHealth>) -> bool {
    health.is_some_and(|health| health.current <= 0.0)
}

fn death_dialog_request(respawn_pending: bool) -> MessageDialogRequest {
    let (body, label) = if respawn_pending {
        (
            "You have died.\n\nRespawn requested — waiting for the server.",
            "Respawning...",
        )
    } else {
        ("You have died.", "OK")
    };
    MessageDialogRequest {
        id: DEATH_RESPAWN_DIALOG_ID,
        title: "Notice".into(),
        body: body.into(),
        buttons: vec![DialogButton::new(label, DialogAction::Confirm)],
        default_action: Some(DialogAction::Confirm),
        cancel_action: None,
        dismissible: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authoritative_dead_uses_replicated_health_only() {
        assert!(!authoritative_dead(None));
        assert!(!authoritative_dead(Some(ReplicatedHealth {
            current: 1.0,
            max: 100.0,
            damage_immunity_active: false,
        })));
        assert!(authoritative_dead(Some(ReplicatedHealth {
            current: 0.0,
            max: 100.0,
            damage_immunity_active: false,
        })));
        assert!(authoritative_dead(Some(ReplicatedHealth {
            current: -3.0,
            max: 100.0,
            damage_immunity_active: false,
        })));
    }

    #[test]
    fn modal_opens_when_dead_and_closes_when_alive() {
        let mut ui = DeathRespawnUi::default();
        let mut dialog = MessageDialog::default();
        ui.sync_modal(&mut dialog, true);
        assert!(dialog.is_active());
        assert!(DeathRespawnUi::is_death_dialog_active(&dialog));

        ui.sync_modal(&mut dialog, false);
        assert!(!dialog.is_active());
        assert!(!ui.respawn_pending());
    }

    #[test]
    fn reconnect_while_dead_reopens_modal() {
        let mut ui = DeathRespawnUi::default();
        let mut dialog = MessageDialog::default();
        ui.sync_modal(&mut dialog, true);
        dialog.close();

        ui.sync_modal(&mut dialog, true);
        assert!(dialog.is_active());
    }

    #[test]
    fn pending_refresh_changes_copy_without_spam_send() {
        let mut ui = DeathRespawnUi::default();
        let mut dialog = MessageDialog::default();
        ui.sync_modal(&mut dialog, true);
        ui.note_respawn_sent();
        ui.sync_modal(&mut dialog, true);
        assert!(dialog.is_active());
        let body = dialog.active_body().expect("body");
        assert!(body.contains("waiting for the server"));
    }

    #[test]
    fn confirm_while_pending_is_not_a_second_send() {
        let result = DeathRespawnUi::interpret_result(
            MessageDialogResult {
                id: DEATH_RESPAWN_DIALOG_ID,
                action: DialogAction::Confirm,
            },
            true,
        );
        assert_eq!(result, DeathRespawnConfirm::IgnoredPending);
    }
}
