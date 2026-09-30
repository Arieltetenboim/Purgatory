//! Persistence worker. Owns identity allocation and character files.
//!
//! The simulation thread hands off owned [`PersistentCharacterSnapshot`] values
//! through a bounded queue with latest-per-character pressure coalescing. JSON
//! and filesystem work happen only on the persistence worker.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use purgatory_common::DevLogin;
use purgatory_persistence::{
    CreateCharacterRejection, DurableCommand, DurableCommandResult, PersistError,
    PersistenceService, PersistentCharacterSnapshot,
};

enum PersistCmd {
    #[allow(dead_code)]
    LoadOwned {
        login: DevLogin,
        character_id: purgatory_common::CharacterId,
        reply: tokio::sync::oneshot::Sender<
            Result<
                purgatory_persistence::PersistentCharacter,
                purgatory_protocol::CharacterEnterRejection,
            >,
        >,
    },
    #[cfg(test)]
    Resolve {
        login: DevLogin,
        reply: tokio::sync::oneshot::Sender<
            Result<purgatory_persistence::PersistentCharacter, PersistError>,
        >,
    },
    Roster {
        login: DevLogin,
        reply: tokio::sync::oneshot::Sender<
            Result<Vec<purgatory_protocol::CharacterSummary>, PersistError>,
        >,
    },
    CreateCharacter {
        login: DevLogin,
        name: String,
        reply: tokio::sync::oneshot::Sender<purgatory_protocol::CreateCharacterResult>,
    },
    #[cfg_attr(not(test), allow(dead_code))]
    CommitDurable {
        command: DurableCommand,
        lease: Option<purgatory_persistence::LeaseAuthority>,
        reply: tokio::sync::oneshot::Sender<Result<DurableCommandResult, PersistError>>,
    },
    ReadOwnedRestore {
        character_id: purgatory_common::CharacterId,
        reply:
            tokio::sync::oneshot::Sender<Result<purgatory_persistence::OwnedRestore, PersistError>>,
    },
    InstallRules {
        rules: purgatory_persistence::DurableContentRules,
        reply: tokio::sync::oneshot::Sender<()>,
    },
    Save {
        snapshot: PersistentCharacterSnapshot,
        lease: Option<purgatory_persistence::LeaseAuthority>,
    },
    SaveAwaited {
        snapshot: PersistentCharacterSnapshot,
        lease: Option<purgatory_persistence::LeaseAuthority>,
        reply: tokio::sync::oneshot::Sender<Result<(), PersistError>>,
    },
    Admit {
        login: DevLogin,
        character_id: purgatory_common::CharacterId,
        reply: tokio::sync::oneshot::Sender<
            Result<purgatory_persistence::SessionAdmission, PersistError>,
        >,
    },
    Supersede {
        authority: purgatory_persistence::LeaseAuthority,
        reply: tokio::sync::oneshot::Sender<
            Result<
                (
                    purgatory_persistence::LeaseAuthority,
                    purgatory_persistence::OwnedRestore,
                ),
                PersistError,
            >,
        >,
    },
    RenewLease {
        authority: purgatory_persistence::LeaseAuthority,
        reply: tokio::sync::oneshot::Sender<Result<(), PersistError>>,
    },
    ReleaseLease {
        authority: purgatory_persistence::LeaseAuthority,
        reply: tokio::sync::oneshot::Sender<Result<(), PersistError>>,
    },
    ClaimChannel {
        channel_id: i64,
        reply:
            tokio::sync::oneshot::Sender<Result<purgatory_persistence::ChannelClaim, PersistError>>,
    },
    RenewChannel {
        channel_id: i64,
        generation: u64,
        reply: tokio::sync::oneshot::Sender<Result<(), PersistError>>,
    },
    #[allow(dead_code)]
    ReleaseChannel {
        channel_id: i64,
        generation: u64,
        reply: tokio::sync::oneshot::Sender<Result<(), PersistError>>,
    },
    Shutdown {
        channel: Option<(i64, u64)>,
        reply: tokio::sync::oneshot::Sender<PersistenceShutdown>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceShutdown {
    /// The worker finished the queued snapshots. `save_failures` counts writes
    /// that did not commit. Zero failures is still not a client-visible "saved"
    /// acknowledgement by itself.
    Drained {
        save_failures: u64,
    },
    /// The drain exceeded its bound. Snapshots still queued were not confirmed.
    TimedOut {
        save_failures: u64,
    },
    WorkerClosed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistenceDiagnosticsSnapshot {
    pub enqueue_accepted: u64,
    pub queue_full: u64,
    pub deferred_latest: u64,
    pub coalesced_replaced: u64,
    pub coalesced_stale_ignored: u64,
    pub worker_closed: u64,
    pub save_failures: u64,
}

#[derive(Default)]
struct PersistenceDiagnostics {
    enqueue_accepted: AtomicU64,
    queue_full: AtomicU64,
    deferred_latest: AtomicU64,
    coalesced_replaced: AtomicU64,
    coalesced_stale_ignored: AtomicU64,
    worker_closed: AtomicU64,
    save_failures: AtomicU64,
}

impl PersistenceDiagnostics {
    fn snapshot(&self) -> PersistenceDiagnosticsSnapshot {
        PersistenceDiagnosticsSnapshot {
            enqueue_accepted: self.enqueue_accepted.load(Ordering::Relaxed),
            queue_full: self.queue_full.load(Ordering::Relaxed),
            deferred_latest: self.deferred_latest.load(Ordering::Relaxed),
            coalesced_replaced: self.coalesced_replaced.load(Ordering::Relaxed),
            coalesced_stale_ignored: self.coalesced_stale_ignored.load(Ordering::Relaxed),
            worker_closed: self.worker_closed.load(Ordering::Relaxed),
            save_failures: self.save_failures.load(Ordering::Relaxed),
        }
    }
}

#[derive(Default)]
struct SharedSaveState {
    latest: Mutex<
        HashMap<
            purgatory_common::CharacterId,
            (
                PersistentCharacterSnapshot,
                Option<purgatory_persistence::LeaseAuthority>,
            ),
        >,
    >,
    diagnostics: PersistenceDiagnostics,
    /// Set when shutdown exceeds its deadline. After a check observes this
    /// flag, the worker does not start another queued save, another deferred
    /// write, or channel release. `flush_deferred_latest` checks before each
    /// deferred write, and `Shutdown` checks again after that flush before
    /// channel release. A write that already passed its check may finish.
    /// `TimedOut` does not confirm it. The flag can become true between
    /// checks, so that already-admitted write may also finish.
    stop_writer: AtomicBool,
    /// Blocks the worker thread inside one command, the way a stalled database
    /// call does. The queue can fill while this receiver is held.
    #[cfg(test)]
    worker_stall: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    #[cfg(test)]
    stall_entered: AtomicBool,
    /// Pauses the first save drained inside `Shutdown`, after that save has
    /// been taken from the queue and before its write returns.
    #[cfg(test)]
    shutdown_save_stall: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    #[cfg(test)]
    shutdown_save_stall_entered: AtomicBool,
    /// Pauses the next deferred write inside `flush_deferred_latest`.
    #[cfg(test)]
    deferred_save_stall: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    #[cfg(test)]
    deferred_save_stall_entered: AtomicBool,
    /// Pauses the next ordinary `Save` after that command was admitted.
    #[cfg(test)]
    command_save_stall: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    #[cfg(test)]
    command_save_stall_entered: AtomicBool,
    #[cfg(test)]
    shutdown_enqueued: AtomicBool,
    #[cfg(test)]
    channel_release_calls: AtomicU64,
    #[cfg(test)]
    release_calls: AtomicU64,
    #[cfg(test)]
    writer_finished: AtomicBool,
    /// Async barrier after the worker admit reply, so `admit().await` stays
    /// pending without blocking the persistence thread.
    #[cfg(test)]
    admit_hold: Mutex<Option<CommandHoldState>>,
    #[cfg(test)]
    renew_channel_hold: Mutex<Option<CommandHoldState>>,
    #[cfg(test)]
    scripted_admit: Mutex<Option<purgatory_persistence::SessionAdmission>>,
}

#[cfg(test)]
fn take_scripted_admit(
    shared: &SharedSaveState,
) -> Option<purgatory_persistence::SessionAdmission> {
    shared
        .scripted_admit
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .take()
}

#[cfg(test)]
struct CommandHoldState {
    entered: Arc<AtomicBool>,
    release: tokio::sync::oneshot::Receiver<()>,
}

#[cfg(test)]
pub struct CommandHold {
    entered: Arc<AtomicBool>,
    release_tx: tokio::sync::oneshot::Sender<()>,
}

#[cfg(test)]
pub struct WorkerStall {
    shared: Arc<SharedSaveState>,
    release_tx: std::sync::mpsc::Sender<()>,
}

#[cfg(test)]
impl WorkerStall {
    pub fn entered(&self) -> bool {
        self.shared.stall_entered.load(Ordering::SeqCst)
    }

    pub fn release(self) {
        let _ = self.release_tx.send(());
    }
}

#[cfg(test)]
enum PauseKind {
    Shutdown,
    Deferred,
    Command,
}

#[cfg(test)]
pub struct ShutdownSaveStall {
    shared: Arc<SharedSaveState>,
    kind: PauseKind,
    release_tx: std::sync::mpsc::Sender<()>,
}

#[cfg(test)]
fn arm_pause(
    shared: &Arc<SharedSaveState>,
    slot: &Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    kind: PauseKind,
) -> ShutdownSaveStall {
    let (release_tx, release) = std::sync::mpsc::channel();
    *slot.lock().unwrap_or_else(|err| err.into_inner()) = Some(release);
    ShutdownSaveStall {
        shared: Arc::clone(shared),
        kind,
        release_tx,
    }
}

#[cfg(test)]
impl ShutdownSaveStall {
    pub fn entered(&self) -> bool {
        let flag = match self.kind {
            PauseKind::Shutdown => &self.shared.shutdown_save_stall_entered,
            PauseKind::Deferred => &self.shared.deferred_save_stall_entered,
            PauseKind::Command => &self.shared.command_save_stall_entered,
        };
        flag.load(Ordering::SeqCst)
    }

    pub fn release(self) {
        let _ = self.release_tx.send(());
    }
}

#[cfg(test)]
impl CommandHold {
    fn pair() -> (Self, CommandHoldState) {
        let (release_tx, release) = tokio::sync::oneshot::channel();
        let entered = Arc::new(AtomicBool::new(false));
        (
            Self {
                entered: entered.clone(),
                release_tx,
            },
            CommandHoldState { entered, release },
        )
    }

    pub fn entered(&self) -> bool {
        self.entered.load(Ordering::SeqCst)
    }

    pub fn release(self) {
        let _ = self.release_tx.send(());
    }
}

#[cfg(test)]
async fn wait_hold(hold: Option<CommandHoldState>) {
    if let Some(hold) = hold {
        hold.entered.store(true, Ordering::SeqCst);
        let _ = hold.release.await;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveHandoff {
    Accepted,
    DeferredLatest,
    Closed,
}

/// Cloneable handle. Connection tasks await roster/create; the sim thread uses a
/// bounded queue and latest-per-character coalescing under pressure.
#[derive(Clone)]
pub struct PersistenceHandle {
    tx: tokio::sync::mpsc::Sender<PersistCmd>,
    shared: Arc<SharedSaveState>,
}

/// Script replies at the persistence-worker queue boundary while exercising
/// the real connection-side durable submission task.
#[cfg(test)]
pub struct ScriptedDurableCalls {
    rx: tokio::sync::mpsc::Receiver<PersistCmd>,
}

#[cfg(test)]
impl ScriptedDurableCalls {
    pub async fn recv(
        &mut self,
    ) -> (
        DurableCommand,
        tokio::sync::oneshot::Sender<Result<DurableCommandResult, PersistError>>,
    ) {
        match self.rx.recv().await.expect("durable submission") {
            PersistCmd::CommitDurable { command, reply, .. } => (command, reply),
            _ => panic!("unexpected persistence command"),
        }
    }
}

impl PersistenceHandle {
    #[cfg(test)]
    pub fn scripted_durable_for_test() -> (Self, ScriptedDurableCalls) {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        (
            Self {
                tx,
                shared: Arc::new(SharedSaveState::default()),
            },
            ScriptedDurableCalls { rx },
        )
    }

    /// Pre-cutover file writer. Tests use this so an ambient database URL cannot
    /// redirect them onto a developer database. The server binary uses
    /// [`Self::spawn_from_env`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn spawn(dir: &Path) -> Result<Self, String> {
        let service =
            PersistenceService::open(dir).map_err(|err| format!("persistence open: {err}"))?;
        Self::spawn_opened(service)
    }

    /// Server startup. PostgreSQL is the only writer when `PURGATORY_DATABASE_URL`
    /// is set. The URL is not assumed to be localhost.
    pub fn spawn_from_env(dir: &Path) -> Result<Self, String> {
        let service = PersistenceService::open_from_env(dir)
            .map_err(|err| format!("persistence open: {err}"))?;
        Self::spawn_opened(service)
    }

    fn spawn_opened(mut service: PersistenceService) -> Result<Self, String> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let shared = Arc::new(SharedSaveState::default());
        let worker_shared = shared.clone();
        std::thread::Builder::new()
            .name("purgatory-persist".into())
            .spawn(move || {
            while let Some(cmd) = rx.blocking_recv() {
                if worker_shared.stop_writer.load(Ordering::SeqCst) {
                    break;
                }
                #[cfg(test)]
                {
                    let stall = worker_shared
                        .worker_stall
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .take();
                    if let Some(stall) = stall {
                        worker_shared.stall_entered.store(true, Ordering::SeqCst);
                        let _ = stall.recv();
                    }
                }
                if worker_shared.stop_writer.load(Ordering::SeqCst) {
                    break;
                }
                match cmd {
                    PersistCmd::LoadOwned {
                        login,
                        character_id,
                        reply,
                    } => {
                        use purgatory_protocol::CharacterEnterRejection as R;
                        let result = service
                            .load_owned_character(&login, character_id)
                            .map_err(|_| R::StorageFailure)
                            .and_then(|v| v.ok_or(R::NotOwned));
                        let _ = reply.send(result);
                    }
                    #[cfg(test)]
                    PersistCmd::Resolve { login, reply } => {
                        let _ = reply.send(service.resolve_or_create(&login));
                    }
                    PersistCmd::Roster { login, reply } => {
                        let _ = reply.send(roster(&mut service, &login));
                    }
                    PersistCmd::CreateCharacter { login, name, reply } => {
                        use purgatory_protocol::{
                            CharacterCreateRejection as Rejection, CreateCharacterResult as Result,
                        };
                        let result = match service.create_character(&login, &name) {
                            Ok(_) => match roster(&mut service, &login) {
                                Ok(roster) => Result::Created { roster },
                                Err(err) => {
                                    eprintln!("PURGATORY character roster read failed: {err}");
                                    Result::Rejected(Rejection::StorageFailure)
                                }
                            },
                            Err(PersistError::CreateRejected(reason)) => {
                                Result::Rejected(match reason {
                                    CreateCharacterRejection::InvalidName(_) => {
                                        Rejection::InvalidName
                                    }
                                    CreateCharacterRejection::NameTaken => Rejection::NameTaken,
                                    CreateCharacterRejection::RosterFull => Rejection::RosterFull,
                                })
                            }
                            Err(err) => {
                                eprintln!("PURGATORY character creation failed: {err}");
                                Result::Rejected(Rejection::StorageFailure)
                            }
                        };
                        let _ = reply.send(result);
                    }
                    PersistCmd::CommitDurable {
                        command,
                        lease,
                        reply,
                    } => {
                        let _ = reply.send(service.commit_durable_leased(&command, lease.as_ref()));
                    }
                    PersistCmd::ReadOwnedRestore {
                        character_id,
                        reply,
                    } => {
                        let _ = reply.send(service.read_owned_restore(character_id));
                    }
                    PersistCmd::InstallRules { rules, reply } => {
                        service.set_durable_content_rules(rules);
                        let _ = reply.send(());
                    }
                    PersistCmd::Save { snapshot, lease } => {
                        #[cfg(test)]
                        pause_receiver(
                            &worker_shared.command_save_stall,
                            &worker_shared.command_save_stall_entered,
                        );
                        save_snapshot_observed(&mut service, &worker_shared, snapshot, lease);
                    }
                    PersistCmd::SaveAwaited {
                        snapshot,
                        lease,
                        reply,
                    } => {
                        let result = service.save_snapshot_leased(snapshot, lease.as_ref());
                        if result.is_err() {
                            worker_shared
                                .diagnostics
                                .save_failures
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        let _ = reply.send(result);
                    }
                    PersistCmd::Admit {
                        login,
                        character_id,
                        reply,
                    } => {
                        #[cfg(test)]
                        let scripted = take_scripted_admit(&worker_shared);
                        #[cfg(test)]
                        let result = match scripted {
                            Some(admission) => Ok(admission),
                            None => service.admit(&login, character_id),
                        };
                        #[cfg(not(test))]
                        let result = service.admit(&login, character_id);
                        let _ = reply.send(result);
                    }
                    PersistCmd::Supersede { authority, reply } => {
                        let _ = reply.send(service.supersede(&authority));
                    }
                    PersistCmd::RenewLease { authority, reply } => {
                        let _ = reply.send(service.renew_lease(&authority));
                    }
                    PersistCmd::ReleaseLease { authority, reply } => {
                        #[cfg(test)]
                        {
                            worker_shared.release_calls.fetch_add(1, Ordering::SeqCst);
                        }
                        let _ = reply.send(service.release_lease(&authority));
                    }
                    PersistCmd::ClaimChannel { channel_id, reply } => {
                        let _ = reply.send(service.claim_channel(channel_id, None));
                    }
                    PersistCmd::RenewChannel {
                        channel_id,
                        generation,
                        reply,
                    } => {
                        let _ = reply.send(service.renew_channel(channel_id, generation));
                    }
                    PersistCmd::ReleaseChannel {
                        channel_id,
                        generation,
                        reply,
                    } => {
                        let _ = reply.send(service.release_channel(channel_id, generation));
                    }
                    PersistCmd::Shutdown { channel, reply } => {
                        while let Ok(extra) = rx.try_recv() {
                            if worker_shared.stop_writer.load(Ordering::SeqCst) {
                                break;
                            }
                            if let PersistCmd::Save { snapshot, lease } = extra {
                                // The check above admitted this save. A timeout
                                // that lands during the write still lets this
                                // call finish; it does not admit the next one.
                                #[cfg(test)]
                                pause_shutdown_save(&worker_shared);
                                save_snapshot_observed(
                                    &mut service,
                                    &worker_shared,
                                    snapshot,
                                    lease,
                                );
                            }
                        }
                        if !worker_shared.stop_writer.load(Ordering::SeqCst) {
                            flush_deferred_latest(&mut service, &worker_shared);
                        }
                        if !worker_shared.stop_writer.load(Ordering::SeqCst) {
                            if let Some((channel_id, generation)) = channel {
                                #[cfg(test)]
                                {
                                    worker_shared
                                        .channel_release_calls
                                        .fetch_add(1, Ordering::SeqCst);
                                }
                                if let Err(err) = service.release_channel(channel_id, generation) {
                                    eprintln!(
                                        "PURGATORY channel release failed id={channel_id} generation={generation}: {err}"
                                    );
                                }
                            }
                            let _ = reply.send(PersistenceShutdown::Drained {
                                save_failures: worker_shared
                                    .diagnostics
                                    .save_failures
                                    .load(Ordering::Relaxed),
                            });
                        }
                        break;
                    }
                }
                flush_deferred_latest(&mut service, &worker_shared);
            }
            if !worker_shared.stop_writer.load(Ordering::SeqCst) {
                flush_deferred_latest(&mut service, &worker_shared);
            }
            #[cfg(test)]
            worker_shared.writer_finished.store(true, Ordering::SeqCst);
        })
            .map_err(|err| format!("persistence worker: {err}"))?;
        Ok(Self { tx, shared })
    }

    #[cfg(test)]
    pub async fn resolve(
        &self,
        login: DevLogin,
    ) -> Result<purgatory_persistence::PersistentCharacter, PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::Resolve { login, reply })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    #[allow(dead_code)]
    pub async fn load_owned_character(
        &self,
        login: DevLogin,
        character_id: purgatory_common::CharacterId,
    ) -> Result<
        purgatory_persistence::PersistentCharacter,
        purgatory_protocol::CharacterEnterRejection,
    > {
        use purgatory_protocol::CharacterEnterRejection as R;
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::LoadOwned {
                login,
                character_id,
                reply,
            })
            .await
            .map_err(|_| R::StorageFailure)?;
        rx.await.map_err(|_| R::StorageFailure)?
    }

    pub async fn roster(
        &self,
        login: DevLogin,
    ) -> Result<Vec<purgatory_protocol::CharacterSummary>, PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::Roster { login, reply })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    pub async fn create_character(
        &self,
        login: DevLogin,
        name: String,
    ) -> purgatory_protocol::CreateCharacterResult {
        let (reply, rx) = tokio::sync::oneshot::channel();
        let failure = purgatory_protocol::CreateCharacterResult::Rejected(
            purgatory_protocol::CharacterCreateRejection::StorageFailure,
        );
        if self
            .tx
            .send(PersistCmd::CreateCharacter { login, name, reply })
            .await
            .is_err()
        {
            return failure;
        }
        rx.await.unwrap_or(failure)
    }

    /// Ask the persistence worker to commit one durable command and wait for
    /// the stored result. A queue send is not success. Callers are connection
    /// tasks; the 30 Hz simulation tick must not call this or block on it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub async fn commit_durable(
        &self,
        command: DurableCommand,
        lease: Option<purgatory_persistence::LeaseAuthority>,
    ) -> Result<DurableCommandResult, PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::CommitDurable {
                command,
                lease,
                reply,
            })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    /// Read committed character state. Callers are connection tasks. The
    /// simulation tick must not call this or block on it.
    pub async fn read_owned_restore(
        &self,
        character_id: purgatory_common::CharacterId,
    ) -> Result<purgatory_persistence::OwnedRestore, PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::ReadOwnedRestore {
                character_id,
                reply,
            })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    /// Install catalog rules before gameplay commands. File mode ignores them.
    pub async fn install_content_rules(
        &self,
        rules: purgatory_persistence::DurableContentRules,
    ) -> Result<(), PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::InstallRules { rules, reply })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn try_save(&self, snapshot: PersistentCharacterSnapshot) -> SaveHandoff {
        self.try_save_leased(snapshot, None)
    }

    /// Queue acceptance is not a durable save.
    pub fn try_save_leased(
        &self,
        snapshot: PersistentCharacterSnapshot,
        lease: Option<purgatory_persistence::LeaseAuthority>,
    ) -> SaveHandoff {
        match self.tx.try_send(PersistCmd::Save { snapshot, lease }) {
            Ok(()) => {
                self.shared
                    .diagnostics
                    .enqueue_accepted
                    .fetch_add(1, Ordering::Relaxed);
                SaveHandoff::Accepted
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(PersistCmd::Save {
                snapshot,
                lease,
            })) => {
                self.shared
                    .diagnostics
                    .queue_full
                    .fetch_add(1, Ordering::Relaxed);
                let mut latest = self
                    .shared
                    .latest
                    .lock()
                    .unwrap_or_else(|err| err.into_inner());
                match latest.entry(snapshot.character_id) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert((snapshot, lease));
                        self.shared
                            .diagnostics
                            .deferred_latest
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        if snapshot.persistence_revision > entry.get().0.persistence_revision {
                            entry.insert((snapshot, lease));
                            self.shared
                                .diagnostics
                                .coalesced_replaced
                                .fetch_add(1, Ordering::Relaxed);
                        } else {
                            self.shared
                                .diagnostics
                                .coalesced_stale_ignored
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                SaveHandoff::DeferredLatest
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(PersistCmd::Save { .. })) => {
                self.shared
                    .diagnostics
                    .worker_closed
                    .fetch_add(1, Ordering::Relaxed);
                SaveHandoff::Closed
            }
            Err(_) => unreachable!("try_save only sends Save commands"),
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> PersistenceDiagnosticsSnapshot {
        self.shared.diagnostics.snapshot()
    }

    #[cfg(test)]
    pub(crate) fn saturated_for_test() -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let shared = Arc::new(SharedSaveState::default());
        let handle = Self { tx, shared };
        let filler = PersistentCharacterSnapshot::from_character(
            &purgatory_persistence::PersistentCharacter::new_default(
                purgatory_common::CharacterId::from_raw(u64::MAX),
            ),
        );
        assert_eq!(handle.try_save(filler), SaveHandoff::Accepted);
        std::mem::forget(rx);
        handle
    }

    #[cfg(test)]
    pub(crate) fn deferred_for_test(
        &self,
        id: purgatory_common::CharacterId,
    ) -> Option<PersistentCharacterSnapshot> {
        self.shared
            .latest
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(&id)
            .map(|(snapshot, _)| snapshot.clone())
    }

    pub async fn save_leased(
        &self,
        snapshot: PersistentCharacterSnapshot,
        lease: Option<purgatory_persistence::LeaseAuthority>,
    ) -> Result<(), PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::SaveAwaited {
                snapshot,
                lease,
                reply,
            })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    pub async fn admit(
        &self,
        login: DevLogin,
        character_id: purgatory_common::CharacterId,
    ) -> Result<purgatory_persistence::SessionAdmission, PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::Admit {
                login,
                character_id,
                reply,
            })
            .await
            .map_err(|_| worker_closed())?;
        let result = rx.await.map_err(|_| worker_closed())?;
        #[cfg(test)]
        {
            let hold = self
                .shared
                .admit_hold
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .take();
            wait_hold(hold).await;
        }
        result
    }

    /// Hold the next `admit().await` after the worker has produced its reply.
    #[cfg(test)]
    pub fn hold_next_admit(&self) -> CommandHold {
        let (hold, state) = CommandHold::pair();
        *self
            .shared
            .admit_hold
            .lock()
            .unwrap_or_else(|err| err.into_inner()) = Some(state);
        hold
    }

    /// Block the worker thread on its next command until [`WorkerStall::release`].
    #[cfg(test)]
    pub fn stall_next_deferred_save(&self) -> ShutdownSaveStall {
        arm_pause(
            &self.shared,
            &self.shared.deferred_save_stall,
            PauseKind::Deferred,
        )
    }

    #[cfg(test)]
    pub fn stall_next_command_save(&self) -> ShutdownSaveStall {
        arm_pause(
            &self.shared,
            &self.shared.command_save_stall,
            PauseKind::Command,
        )
    }

    #[cfg(test)]
    pub fn stall_next_shutdown_save(&self) -> ShutdownSaveStall {
        arm_pause(
            &self.shared,
            &self.shared.shutdown_save_stall,
            PauseKind::Shutdown,
        )
    }

    #[cfg(test)]
    pub fn shutdown_enqueued_for_test(&self) -> bool {
        self.shared.shutdown_enqueued.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn channel_release_calls_for_test(&self) -> u64 {
        self.shared.channel_release_calls.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn defer_latest_for_test(&self, snapshot: PersistentCharacterSnapshot) {
        self.shared
            .latest
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(snapshot.character_id, (snapshot, None));
    }

    #[cfg(test)]
    pub fn stall_next_command(&self) -> WorkerStall {
        let (release_tx, release) = std::sync::mpsc::channel();
        *self
            .shared
            .worker_stall
            .lock()
            .unwrap_or_else(|err| err.into_inner()) = Some(release);
        WorkerStall {
            shared: Arc::clone(&self.shared),
            release_tx,
        }
    }

    #[cfg(test)]
    pub fn release_calls_for_test(&self) -> u64 {
        self.shared.release_calls.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn writer_finished_for_test(&self) -> bool {
        self.shared.writer_finished.load(Ordering::SeqCst)
    }

    /// Replace the next admit result. File mode has no lease; tests that need
    /// a character deadline install one here. The handoff after the reply is
    /// still `activate_owned_character`.
    #[cfg(test)]
    pub fn script_next_admit(&self, admission: purgatory_persistence::SessionAdmission) {
        *self
            .shared
            .scripted_admit
            .lock()
            .unwrap_or_else(|err| err.into_inner()) = Some(admission);
    }

    /// Hold the next channel renewal reply so the supervisor stays inside the
    /// renewal future until the local deadline wins.
    #[cfg(test)]
    pub fn hold_next_channel_renewal(&self) -> CommandHold {
        let (hold, state) = CommandHold::pair();
        *self
            .shared
            .renew_channel_hold
            .lock()
            .unwrap_or_else(|err| err.into_inner()) = Some(state);
        hold
    }

    pub async fn supersede(
        &self,
        authority: purgatory_persistence::LeaseAuthority,
    ) -> Result<
        (
            purgatory_persistence::LeaseAuthority,
            purgatory_persistence::OwnedRestore,
        ),
        PersistError,
    > {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::Supersede { authority, reply })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    pub async fn renew_lease(
        &self,
        authority: purgatory_persistence::LeaseAuthority,
    ) -> Result<(), PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::RenewLease { authority, reply })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    pub async fn release_lease(
        &self,
        authority: purgatory_persistence::LeaseAuthority,
    ) -> Result<(), PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::ReleaseLease { authority, reply })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    pub async fn claim_channel(
        &self,
        channel_id: i64,
    ) -> Result<purgatory_persistence::ChannelClaim, PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::ClaimChannel { channel_id, reply })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    pub async fn renew_channel(
        &self,
        channel_id: i64,
        generation: u64,
    ) -> Result<(), PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::RenewChannel {
                channel_id,
                generation,
                reply,
            })
            .await
            .map_err(|_| worker_closed())?;
        let result = rx.await.map_err(|_| worker_closed())?;
        #[cfg(test)]
        {
            let hold = self
                .shared
                .renew_channel_hold
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .take();
            wait_hold(hold).await;
        }
        result
    }

    #[allow(dead_code)]
    pub async fn release_channel(
        &self,
        channel_id: i64,
        generation: u64,
    ) -> Result<(), PersistError> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PersistCmd::ReleaseChannel {
                channel_id,
                generation,
                reply,
            })
            .await
            .map_err(|_| worker_closed())?;
        rx.await.map_err(|_| worker_closed())?
    }

    pub async fn shutdown(
        self,
        timeout: Duration,
        channel: Option<(i64, u64)>,
    ) -> PersistenceShutdown {
        let (reply, rx) = tokio::sync::oneshot::channel();
        let tx = self.tx.clone();
        let shared = Arc::clone(&self.shared);
        #[cfg(test)]
        let enqueued = Arc::clone(&shared);
        let send_and_wait = async move {
            tx.send(PersistCmd::Shutdown { channel, reply })
                .await
                .map_err(|_| ())?;
            #[cfg(test)]
            enqueued.shutdown_enqueued.store(true, Ordering::SeqCst);
            rx.await.map_err(|_| ())
        };
        match tokio::time::timeout(timeout, send_and_wait).await {
            Ok(Ok(status)) => status,
            Ok(Err(())) => PersistenceShutdown::WorkerClosed,
            Err(_) => {
                shared.stop_writer.store(true, Ordering::SeqCst);
                PersistenceShutdown::TimedOut {
                    save_failures: shared.diagnostics.save_failures.load(Ordering::Relaxed),
                }
            }
        }
    }
}

#[cfg(test)]
fn pause_receiver(slot: &Mutex<Option<std::sync::mpsc::Receiver<()>>>, entered: &AtomicBool) {
    let stall = slot.lock().unwrap_or_else(|err| err.into_inner()).take();
    if let Some(stall) = stall {
        entered.store(true, Ordering::SeqCst);
        let _ = stall.recv();
    }
}

#[cfg(test)]
fn pause_shutdown_save(shared: &SharedSaveState) {
    pause_receiver(
        &shared.shutdown_save_stall,
        &shared.shutdown_save_stall_entered,
    );
}

fn save_snapshot_observed(
    service: &mut PersistenceService,
    shared: &SharedSaveState,
    snapshot: PersistentCharacterSnapshot,
    lease: Option<purgatory_persistence::LeaseAuthority>,
) {
    if let Err(err) = service.save_snapshot_leased(snapshot, lease.as_ref()) {
        shared
            .diagnostics
            .save_failures
            .fetch_add(1, Ordering::Relaxed);
        eprintln!("PURGATORY persist save failed: {err}");
    }
}

fn flush_deferred_latest(service: &mut PersistenceService, shared: &SharedSaveState) {
    let pending = {
        let mut latest = shared.latest.lock().unwrap_or_else(|err| err.into_inner());
        latest.drain().map(|(_, saved)| saved).collect::<Vec<_>>()
    };
    for (snapshot, lease) in pending {
        if shared.stop_writer.load(Ordering::SeqCst) {
            break;
        }
        // This write already passed the check. A timeout during it still
        // finishes this call and does not admit the next deferred write.
        #[cfg(test)]
        pause_receiver(
            &shared.deferred_save_stall,
            &shared.deferred_save_stall_entered,
        );
        save_snapshot_observed(service, shared, snapshot, lease);
    }
}

#[must_use]
pub fn data_dir_from_env() -> PathBuf {
    resolve_data_dir(|key| std::env::var(key))
}

/// Resolve the runtime persistence root.
///
/// `PURGATORY_DATA_DIR` wins when set and non-empty. Otherwise the default is
/// a per-user application-data directory, never the source/install tree.
pub(crate) fn resolve_data_dir(
    mut getenv: impl FnMut(&str) -> Result<String, std::env::VarError>,
) -> PathBuf {
    if let Ok(dir) = getenv("PURGATORY_DATA_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    default_data_dir(getenv)
}

fn default_data_dir(mut getenv: impl FnMut(&str) -> Result<String, std::env::VarError>) -> PathBuf {
    #[cfg(windows)]
    {
        getenv("LOCALAPPDATA")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Purgatory")
    }
    #[cfg(not(windows))]
    {
        if let Ok(xdg) = getenv("XDG_DATA_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("purgatory");
            }
        }
        getenv("HOME")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|home| {
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("purgatory")
            })
            .unwrap_or_else(|| PathBuf::from("/var/tmp/purgatory"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::VarError;
    use std::path::PathBuf;

    fn env_map<'a>(
        pairs: &'a [(&'a str, &'a str)],
    ) -> impl FnMut(&str) -> Result<String, VarError> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
                .ok_or(VarError::NotPresent)
        }
    }

    #[test]
    fn durable_slots_match_the_simulation_contracts() {
        assert_eq!(
            usize::from(purgatory_persistence::DURABLE_INVENTORY_CAPACITY),
            purgatory_simulation::INVENTORY_CAPACITY
        );
        for slot in purgatory_simulation::EquipmentSlot::ALL {
            let durable = purgatory_persistence::DurableEquipmentSlot::parse(slot.as_str())
                .expect("durable equipment slot name");
            assert_eq!(durable.as_str(), slot.as_str());
        }
    }

    #[test]
    fn override_wins_over_platform_default() {
        let dir = resolve_data_dir(env_map(&[
            ("PURGATORY_DATA_DIR", r"D:\forced\persist"),
            ("LOCALAPPDATA", r"C:\should-not-use"),
            ("XDG_DATA_HOME", "/should-not-use"),
            ("HOME", "/should-not-use"),
        ]));
        assert_eq!(dir, PathBuf::from(r"D:\forced\persist"));
    }

    #[test]
    fn empty_override_falls_through() {
        let dir = resolve_data_dir(env_map(&[("PURGATORY_DATA_DIR", "  ")]));
        assert_ne!(dir, PathBuf::from("data"));
        assert_ne!(dir.as_os_str(), "data");
    }

    #[test]
    fn default_is_never_repo_relative_data() {
        let dir = resolve_data_dir(env_map(&[]));
        assert_ne!(dir, PathBuf::from("data"));
        assert_ne!(dir.as_os_str(), "data");
    }

    #[cfg(windows)]
    #[test]
    fn windows_default_is_localappdata_purgatory() {
        let dir = resolve_data_dir(env_map(&[("LOCALAPPDATA", r"C:\fake-local")]));
        assert_eq!(dir, PathBuf::from(r"C:\fake-local\Purgatory"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_missing_localappdata_is_still_not_repo_data() {
        let dir = resolve_data_dir(env_map(&[]));
        assert_eq!(dir, PathBuf::from(".").join("Purgatory"));
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_default_prefers_xdg_then_home() {
        let xdg = resolve_data_dir(env_map(&[("XDG_DATA_HOME", "/xdg/data")]));
        assert_eq!(xdg, PathBuf::from("/xdg/data/purgatory"));
        let home = resolve_data_dir(env_map(&[("HOME", "/home/dev")]));
        assert_eq!(home, PathBuf::from("/home/dev/.local/share/purgatory"));
    }

    #[test]
    fn saturation_keeps_latest_snapshot_per_character_and_counts_pressure() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let shared = Arc::new(SharedSaveState::default());
        let handle = PersistenceHandle {
            tx,
            shared: shared.clone(),
        };
        let id = purgatory_common::CharacterId::from_raw(31);
        let mut character = purgatory_persistence::PersistentCharacter::new_default(id);
        character.persistence_revision = 1;
        let first = PersistentCharacterSnapshot::from_character(&character);
        assert_eq!(handle.try_save(first), SaveHandoff::Accepted);

        character.persistence_revision = 2;
        let second = PersistentCharacterSnapshot::from_character(&character);
        assert_eq!(handle.try_save(second), SaveHandoff::DeferredLatest);

        character.persistence_revision = 3;
        let third = PersistentCharacterSnapshot::from_character(&character);
        assert_eq!(handle.try_save(third), SaveHandoff::DeferredLatest);

        character.persistence_revision = 2;
        let stale = PersistentCharacterSnapshot::from_character(&character);
        assert_eq!(handle.try_save(stale), SaveHandoff::DeferredLatest);

        let queued = match rx.try_recv().unwrap() {
            PersistCmd::Save {
                snapshot,
                lease: None,
            } => snapshot,
            _ => panic!("expected save"),
        };
        assert_eq!(queued.persistence_revision, 1);
        let deferred = handle.deferred_for_test(id).unwrap();
        assert_eq!(deferred.persistence_revision, 3);

        assert_eq!(
            handle.diagnostics(),
            PersistenceDiagnosticsSnapshot {
                enqueue_accepted: 1,
                queue_full: 3,
                deferred_latest: 1,
                coalesced_replaced: 1,
                coalesced_stale_ignored: 1,
                worker_closed: 0,
                save_failures: 0,
            }
        );
    }

    #[test]
    fn closed_worker_is_explicit_and_counted() {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        drop(rx);
        let handle = PersistenceHandle {
            tx,
            shared: Arc::new(SharedSaveState::default()),
        };
        let id = purgatory_common::CharacterId::from_raw(32);
        let snapshot = PersistentCharacterSnapshot::from_character(
            &purgatory_persistence::PersistentCharacter::new_default(id),
        );
        assert_eq!(handle.try_save(snapshot), SaveHandoff::Closed);
        assert_eq!(handle.diagnostics().worker_closed, 1);
    }

    #[tokio::test]
    async fn shutdown_flushes_deferred_latest_state() {
        let dir = std::env::temp_dir().join(format!(
            "purgatory-persist-deferred-shutdown-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let handle = PersistenceHandle::spawn(&dir).expect("spawn");
        let id = purgatory_common::CharacterId::from_raw(33);
        let mut character = purgatory_persistence::PersistentCharacter::new_default(id);
        character.persistence_revision = 9;
        let snapshot = PersistentCharacterSnapshot::from_character(&character);
        handle
            .shared
            .latest
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(id, (snapshot, None));

        handle.shutdown(Duration::from_secs(2), None).await;

        let repo = purgatory_persistence::FileCharacterRepository::open(&dir).unwrap();
        let loaded = repo.load(id).unwrap().unwrap();
        assert_eq!(loaded.persistence_revision, 9);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn shutdown_drains_pending_saves_in_temp_dir() {
        let dir = std::env::temp_dir().join(format!(
            "purgatory-persist-drain-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().to_ascii_lowercase();
        assert!(!text.contains("localappdata"));
        assert!(!text.contains("appdata\\purgatory"));

        let handle = PersistenceHandle::spawn(&dir).expect("spawn");
        let id = purgatory_common::CharacterId::from_raw(21);
        let character = purgatory_persistence::PersistentCharacter::new_default(id);
        let snapshot =
            purgatory_persistence::PersistentCharacterSnapshot::from_character(&character);
        assert_eq!(handle.try_save(snapshot), SaveHandoff::Accepted);
        handle.shutdown(Duration::from_secs(2), None).await;
        let path = dir.join(purgatory_persistence::character_file_name(id));
        assert!(
            path.exists(),
            "shutdown must drain the pending save into the temp tree"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shutdown_times_out_within_the_total_deadline_while_the_worker_is_stalled() {
        if std::env::var_os("PURGATORY_SHUTDOWN_STALL_CHILD").is_some() {
            stalled_shutdown_child();
            return;
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("network::persist::tests::shutdown_times_out_within_the_total_deadline_while_the_worker_is_stalled")
            .arg("--exact")
            .env("PURGATORY_SHUTDOWN_STALL_CHILD", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        let limit = std::time::Duration::from_secs(3);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut out) = child.stdout.take() {
                    use std::io::Read;
                    out.read_to_string(&mut stdout).unwrap();
                }
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    err.read_to_string(&mut stderr).unwrap();
                }
                assert!(
                    status.success(),
                    "stalled shutdown child failed: {status:?}\n{stdout}\n{stderr}"
                );
                return;
            }
            if started.elapsed() > limit {
                let _ = child.kill();
                panic!("shutdown did not finish within {limit:?} while the worker stayed stalled");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_timeout_during_drain_does_not_start_later_writes() {
        let dir = std::env::temp_dir().join(format!(
            "purgatory-shutdown-drain-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let handle = PersistenceHandle::spawn(&dir).unwrap();
        let outer = handle.stall_next_command();
        let placeholder = snapshot_for(41);
        assert_eq!(handle.try_save(placeholder), SaveHandoff::Accepted);
        for _ in 0..50 {
            if outer.entered() {
                break;
            }
            tokio::task::yield_now().await;
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(outer.entered(), "worker did not stall before Shutdown");

        let shutdown_handle = handle.clone();
        let shutdown = tokio::spawn(async move {
            shutdown_handle
                .shutdown(std::time::Duration::from_millis(200), Some((7, 1)))
                .await
        });
        for _ in 0..50 {
            if handle.shutdown_enqueued_for_test() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(
            handle.shutdown_enqueued_for_test(),
            "Shutdown was not queued while the worker was stalled"
        );
        assert_eq!(handle.try_save(snapshot_for(100)), SaveHandoff::Accepted);
        assert_eq!(handle.try_save(snapshot_for(101)), SaveHandoff::Accepted);
        let inner = handle.stall_next_shutdown_save();
        outer.release();
        for _ in 0..50 {
            if inner.entered() {
                break;
            }
            tokio::task::yield_now().await;
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            inner.entered(),
            "the first save inside Shutdown was not paused"
        );
        handle.defer_latest_for_test(snapshot_for(102));

        let status = shutdown.await.unwrap();
        assert!(
            matches!(status, PersistenceShutdown::TimedOut { .. }),
            "shutdown confirmed work that was still inside Shutdown: {status:?}"
        );
        let later = dir.join(purgatory_persistence::character_file_name(
            purgatory_common::CharacterId::from_raw(101),
        ));
        let deferred = dir.join(purgatory_persistence::character_file_name(
            purgatory_common::CharacterId::from_raw(102),
        ));
        assert!(
            !later.exists(),
            "a later queued save was written after timeout"
        );
        assert!(
            !deferred.exists(),
            "deferred state was flushed after timeout"
        );
        assert_eq!(handle.channel_release_calls_for_test(), 0);

        inner.release();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while !handle.writer_finished_for_test() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(handle.writer_finished_for_test(), "writer did not stop");
        let started = dir.join(purgatory_persistence::character_file_name(
            purgatory_common::CharacterId::from_raw(100),
        ));
        assert!(
            started.exists(),
            "the save already inside Shutdown did not finish"
        );
        assert!(
            !later.exists(),
            "a later queued save started after Shutdown timed out"
        );
        assert!(
            !deferred.exists(),
            "deferred state was written after Shutdown timed out"
        );
        assert_eq!(
            handle.channel_release_calls_for_test(),
            0,
            "channel release started after Shutdown timed out"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_timeout_during_deferred_flush_does_not_start_later_writes() {
        let dir = temp_persist_dir("purgatory-shutdown-deferred");
        let handle = PersistenceHandle::spawn(&dir).unwrap();
        let outer = handle.stall_next_command();
        let shutdown_handle = handle.clone();
        let shutdown = tokio::spawn(async move {
            shutdown_handle
                .shutdown(std::time::Duration::from_millis(300), Some((7, 1)))
                .await
        });
        wait_flag(
            || outer.entered() && handle.shutdown_enqueued_for_test(),
            "worker did not stall on Shutdown before the deferred flush",
        )
        .await;
        handle.defer_latest_for_test(snapshot_for(200));
        handle.defer_latest_for_test(snapshot_for(201));
        let deferred = handle.stall_next_deferred_save();
        outer.release();
        wait_flag(
            || deferred.entered(),
            "the first deferred write inside Shutdown was not paused",
        )
        .await;

        let status = shutdown.await.unwrap();
        assert!(
            matches!(status, PersistenceShutdown::TimedOut { .. }),
            "shutdown confirmed a deferred flush still in progress: {status:?}"
        );
        deferred.release();
        wait_writer(&handle);
        let written = [200u64, 201]
            .into_iter()
            .filter(|id| character_path(&dir, *id).exists())
            .count();
        assert_eq!(
            written,
            1,
            "deferred files after timeout: 200={} 201={}",
            character_path(&dir, 200).exists(),
            character_path(&dir, 201).exists()
        );
        assert_eq!(
            handle.channel_release_calls_for_test(),
            0,
            "channel release started after a deferred flush timed out"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_timeout_during_command_skips_post_command_deferred_flush() {
        let dir = temp_persist_dir("purgatory-shutdown-post-flush");
        let handle = PersistenceHandle::spawn(&dir).unwrap();
        let command = handle.stall_next_command_save();
        assert_eq!(handle.try_save(snapshot_for(300)), SaveHandoff::Accepted);
        wait_flag(
            || command.entered(),
            "the ordinary save was not paused before its deferred flush",
        )
        .await;
        handle.defer_latest_for_test(snapshot_for(301));
        handle.defer_latest_for_test(snapshot_for(302));
        let shutdown_handle = handle.clone();
        let status = tokio::spawn(async move {
            shutdown_handle
                .shutdown(std::time::Duration::from_millis(200), Some((8, 1)))
                .await
        })
        .await
        .unwrap();
        assert!(
            matches!(status, PersistenceShutdown::TimedOut { .. }),
            "shutdown confirmed the ordinary command: {status:?}"
        );
        command.release();
        wait_writer(&handle);
        assert!(
            character_path(&dir, 300).exists(),
            "the save already inside the ordinary command did not finish"
        );
        assert!(
            !character_path(&dir, 301).exists() && !character_path(&dir, 302).exists(),
            "post-command deferred flush wrote after timeout: 301={} 302={}",
            character_path(&dir, 301).exists(),
            character_path(&dir, 302).exists()
        );
        assert_eq!(handle.channel_release_calls_for_test(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
fn temp_persist_dir(prefix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
fn character_path(dir: &std::path::Path, id: u64) -> std::path::PathBuf {
    dir.join(purgatory_persistence::character_file_name(
        purgatory_common::CharacterId::from_raw(id),
    ))
}

#[cfg(test)]
async fn wait_flag(mut ready: impl FnMut() -> bool, message: &str) {
    for _ in 0..50 {
        if ready() {
            return;
        }
        tokio::task::yield_now().await;
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("{message}");
}

#[cfg(test)]
fn wait_writer(handle: &PersistenceHandle) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !handle.writer_finished_for_test() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(handle.writer_finished_for_test(), "writer did not stop");
}

#[cfg(test)]
fn snapshot_for(id: u64) -> PersistentCharacterSnapshot {
    PersistentCharacterSnapshot::from_character(
        &purgatory_persistence::PersistentCharacter::new_default(
            purgatory_common::CharacterId::from_raw(id),
        ),
    )
}

#[cfg(test)]
fn stalled_shutdown_child() {
    let dir = std::env::temp_dir().join(format!(
        "purgatory-shutdown-stall-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (stall, observe) = runtime.block_on(async {
        let handle = PersistenceHandle::spawn(&dir).unwrap();
        let stall = handle.stall_next_command();
        let stalled_id = purgatory_common::CharacterId::from_raw(41);
        let stalled = purgatory_persistence::PersistentCharacterSnapshot::from_character(
            &purgatory_persistence::PersistentCharacter::new_default(stalled_id),
        );
        assert_eq!(handle.try_save(stalled), SaveHandoff::Accepted);
        for _ in 0..50 {
            if stall.entered() {
                break;
            }
            tokio::task::yield_now().await;
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(stall.entered(), "worker did not enter the stall");
        let mut accepted = 0u32;
        for n in 0..80u64 {
            let id = purgatory_common::CharacterId::from_raw(1_000 + n);
            let snapshot = purgatory_persistence::PersistentCharacterSnapshot::from_character(
                &purgatory_persistence::PersistentCharacter::new_default(id),
            );
            match handle.try_save(snapshot) {
                SaveHandoff::Accepted => accepted += 1,
                SaveHandoff::DeferredLatest => break,
                SaveHandoff::Closed => panic!("worker closed while filling the queue"),
            }
        }
        assert_eq!(accepted, 64, "the bounded queue was not full");
        let timeout = std::time::Duration::from_millis(200);
        let started = std::time::Instant::now();
        let status = handle.clone().shutdown(timeout, None).await;
        let elapsed = started.elapsed();
        assert!(
            matches!(status, PersistenceShutdown::TimedOut { .. }),
            "shutdown status while the queue was full: {status:?}"
        );
        assert!(
            elapsed <= timeout + std::time::Duration::from_millis(250),
            "shutdown waited {elapsed:?}, past the {timeout:?} deadline"
        );
        let queued = dir.join(purgatory_persistence::character_file_name(
            purgatory_common::CharacterId::from_raw(1_000),
        ));
        assert!(
            !queued.exists(),
            "a queued save was written during the stall"
        );
        (stall, handle)
    });
    drop(runtime);
    stall.release();
    let observe_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !observe.writer_finished_for_test() && std::time::Instant::now() < observe_deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        observe.writer_finished_for_test(),
        "the writer kept running after shutdown timed out"
    );
    let queued = dir.join(purgatory_persistence::character_file_name(
        purgatory_common::CharacterId::from_raw(1_000),
    ));
    assert!(
        !queued.exists(),
        "the writer committed a queued save after shutdown timed out"
    );
    let _ = std::fs::remove_dir_all(&dir);
    std::process::exit(0);
}

fn worker_closed() -> PersistError {
    PersistError::corrupt(PathBuf::from("<worker>"), "persistence worker closed")
}

fn roster(
    service: &mut PersistenceService,
    login: &DevLogin,
) -> Result<Vec<purgatory_protocol::CharacterSummary>, PersistError> {
    Ok(service
        .roster(login)?
        .into_iter()
        .map(|entry| purgatory_protocol::CharacterSummary {
            character_id: entry.character_id,
            display_name: entry.display_name.as_str().to_owned(),
        })
        .collect())
}

#[cfg(test)]
mod frontend_worker_tests {
    use super::*;
    use purgatory_protocol::{
        CharacterCreateRejection as Rejection, CreateCharacterResult as Result,
    };

    #[tokio::test]
    async fn load_owned_corrupt_character_maps_to_storage_failure_and_preserves_bytes() {
        let dir = std::env::temp_dir().join(format!(
            "purgatory-r5b-load-corrupt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let worker = PersistenceHandle::spawn(&dir).unwrap();
        let login = DevLogin::parse("alice").unwrap();
        let Result::Created { roster } = worker
            .create_character(login.clone(), "CorruptHero".into())
            .await
        else {
            panic!("create");
        };
        let id = roster[0].character_id;
        let path = dir.join(purgatory_persistence::character_file_name(id));
        let original = b"{not json".to_vec();
        std::fs::write(&path, &original).unwrap();

        assert_eq!(
            worker.load_owned_character(login, id).await,
            Err(purgatory_protocol::CharacterEnterRejection::StorageFailure)
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);

        worker.shutdown(Duration::from_secs(2), None).await;
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn frontend_storage_failure_has_no_projection_mutation_and_restart_keeps_order() {
        let dir = std::env::temp_dir().join(format!(
            "purgatory-r5b-worker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let worker = PersistenceHandle::spawn(&dir).unwrap();
        let login = DevLogin::parse("alice").unwrap();
        assert!(worker.roster(login.clone()).await.unwrap().is_empty());
        let Result::Created { roster: first } = worker
            .create_character(login.clone(), "FirstHero".into())
            .await
        else {
            panic!("first");
        };
        let obstacle = dir.join("identity.json.tmp");
        std::fs::create_dir(&obstacle).unwrap();
        assert_eq!(
            worker
                .create_character(login.clone(), "NextHero".into())
                .await,
            Result::Rejected(Rejection::StorageFailure)
        );
        assert_eq!(worker.roster(login.clone()).await.unwrap(), first);
        std::fs::remove_dir(&obstacle).unwrap();
        let Result::Created { roster: second } = worker
            .create_character(login.clone(), "NextHero".into())
            .await
        else {
            panic!("retry");
        };
        assert_eq!(second[0], first[0]);
        assert_eq!(
            second[1].character_id.raw(),
            first[0].character_id.raw() + 1
        );
        worker.shutdown(Duration::from_secs(2), None).await;
        let worker = PersistenceHandle::spawn(&dir).unwrap();
        assert_eq!(worker.roster(login).await.unwrap(), second);
        worker.shutdown(Duration::from_secs(2), None).await;
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn durable_command_waits_for_the_worker_result() {
        let dir = std::env::temp_dir().join(format!(
            "purgatory-12a-worker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let worker = PersistenceHandle::spawn(&dir).unwrap();
        let err = worker
            .commit_durable(
                DurableCommand {
                    key: "not-wired".into(),
                    expected_revisions: Vec::new(),
                    place_new: Vec::new(),
                    moves: Vec::new(),
                    retire: Vec::new(),
                    narrative: Vec::new(),
                    learned: Vec::new(),
                },
                None,
            )
            .await
            .expect_err("file mode has no durable command result");
        let text = err.to_string();
        assert!(
            text.contains("postgresql"),
            "the worker must return the service result, not a handoff: {text}"
        );
        worker.shutdown(Duration::from_secs(2), None).await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
