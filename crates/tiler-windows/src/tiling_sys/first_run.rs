use super::*;

/// Deferred first-run inputs for the normal `tile` owner only. The parsed
/// options plus the pre-lease settings state travel into the owned loop and
/// resolve under the lease before any effect or hook.
pub(super) struct FirstRunPending {
    pub(super) options: TileOptions,
    pub(super) live: Option<LiveSettings>,
    pub(super) settings_dir: Option<std::path::PathBuf>,
}

/// Resolved first-run state for the normal `tile` owner: the effective
/// options plus the live settings state, directory, and `tile-start` note.
#[derive(Debug, Clone)]
pub struct FirstRunResolution {
    pub options: TileOptions,
    pub live: Option<LiveSettings>,
    pub settings_dir: Option<std::path::PathBuf>,
    pub first_run: Option<String>,
}

/// Carry the CLI/run-only lanes over a settings base: explicit CLI switches
/// re-apply over the base, and seconds/trace/scope plus the user-start fence
/// (never from the file) carry over from the parsed options.
fn options_with_cli(
    base_settings: &crate::settings::Settings,
    cli_overrides: &crate::tiling::CliOverrides,
    run_options: &TileOptions,
) -> TileOptions {
    let mut base = crate::settings::tile_options_from_settings(base_settings);
    crate::tiling::apply_cli_overrides(&mut base, cli_overrides);
    base.seconds = run_options.seconds;
    base.trace = run_options.trace;
    base.user_start = run_options.user_start;
    base.scope_exes = run_options.scope_exes.clone();
    base.scope_hosts = run_options.scope_hosts.clone();
    base
}

/// Authoritative base when the prompt choice is discarded (a file appeared
/// mid-prompt, the create race was lost, or the choice failed validation):
/// the file wins when it loads, otherwise last-good defaults stay live with
/// a degraded status. Explicit CLI switches and run-only lanes re-apply over
/// either base. Nothing is written.
fn authoritative_base(
    dir: &std::path::Path,
    run_options: &TileOptions,
    cli_overrides: &crate::tiling::CliOverrides,
) -> (TileOptions, Option<LiveSettings>) {
    let mtime = std::fs::metadata(dir.join(crate::settings::SETTINGS_FILE_NAME))
        .and_then(|meta| meta.modified())
        .ok();
    match crate::settings::load_from_dir(dir) {
        crate::settings::LoadOutcome::Loaded(settings) => {
            let options = options_with_cli(&settings, cli_overrides, run_options);
            (options, Some(LiveSettings::fresh(settings, mtime)))
        }
        crate::settings::LoadOutcome::Missing => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = "missing: defaults".to_owned();
            let options = options_with_cli(&live.settings.clone(), cli_overrides, run_options);
            (options, Some(live))
        }
        crate::settings::LoadOutcome::Invalid(error) => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = format!("degraded:{error}");
            let options = options_with_cli(&live.settings.clone(), cli_overrides, run_options);
            (options, Some(live))
        }
    }
}

/// Lease-held first-run resolution for the normal `tile` owner only. The
/// caller holds the single-owner lease, so a second owner never reaches
/// here (its lease refusal lands before any UI). A stop racing startup
/// skips the modal UI and the loop exits promptly; a stop arriving while the
/// prompt is pending dismisses it and publishes nothing. After the choice,
/// the file is rechecked: a file that appeared mid-prompt discards the choice
/// and reloads the authoritative base without writing. An absent file
/// persists through the atomic create-if-absent publish: the winner's bytes
/// land complete, a lost race discards the choice and reloads instead of
/// overwriting, and a plain I/O failure keeps the choice in memory for this
/// run only with an explicit unsaved status plus a `first-run-save-failed`
/// line (never a silent success claim).
pub(super) fn resolve_first_run(
    dir: &Path,
    me: &ProcessIdentity,
    pending: FirstRunPending,
    cli_overrides: &crate::tiling::CliOverrides,
    log_path: &Path,
) -> Result<FirstRunResolution> {
    let FirstRunPending {
        options,
        live,
        settings_dir,
    } = pending;
    let done = |options, live, settings_dir, first_run| FirstRunResolution {
        options,
        live,
        settings_dir,
        first_run,
    };
    let Some(sdir) = settings_dir.clone() else {
        return Ok(done(options, live, settings_dir, None));
    };
    let file_missing_before = matches!(
        crate::settings::load_from_dir(&sdir),
        crate::settings::LoadOutcome::Missing
    );
    if !crate::tray::first_run_should_prompt(file_missing_before, stop_requested(dir, me)?) {
        return Ok(done(options, live, settings_dir, None));
    }
    // Owned cancellable dialog under the lease: a graceful stop while pending
    // destroys the owned window (stop wins) and publishes nothing below.
    let outcome = crate::tray_sys::first_run_dialog(|| stop_requested(dir, me).unwrap_or(false));
    let stop_during = stop_requested(dir, me)?;
    let file_missing_after = matches!(
        crate::settings::load_from_dir(&sdir),
        crate::settings::LoadOutcome::Missing
    );
    resolve_first_run_outcome(
        &sdir,
        outcome.choice(),
        file_missing_after,
        stop_during,
        &options,
        cli_overrides,
        live,
        settings_dir,
        log_path,
    )
}

/// Lease-held first-run outcome application: the write-capable production
/// path shared by the owner and offline tests. `choice` is the dialog result
/// (`None` when dismissed), `file_missing_after` the lease-held recheck, and
/// `stop_during` the lease-held stop recheck. A stop wins over any pending
/// choice (nothing published); a mid-prompt file discards the choice without
/// overwriting; an absent file publishes the choice through the atomic
/// create-if-absent path. Every branch reports its `tile-start` note.
#[allow(clippy::too_many_arguments)]
pub fn resolve_first_run_outcome(
    sdir: &Path,
    choice: Option<crate::tray::FirstRunChoice>,
    file_missing_after: bool,
    stop_during: bool,
    run_options: &TileOptions,
    cli_overrides: &crate::tiling::CliOverrides,
    live: Option<LiveSettings>,
    settings_dir: Option<std::path::PathBuf>,
    log_path: &Path,
) -> Result<FirstRunResolution> {
    let done = |options, live, settings_dir, first_run| FirstRunResolution {
        options,
        live,
        settings_dir,
        first_run,
    };
    // The pending inputs travel by value: discarding branches rebuild the
    // authoritative base from the file, publishing branches derive from it.
    let options = run_options.clone();
    match crate::tray::first_run_post_decision(choice, file_missing_after, stop_during) {
        crate::tray::FirstRunPostDecision::CancelledStopped => {
            log_json_at(
                log_path,
                serde_json::json!({"event": "first-run-cancelled", "reason": "stopped"}),
            );
            Ok(done(
                options,
                live,
                settings_dir,
                Some("first-run:cancelled:stopped".to_owned()),
            ))
        }
        crate::tray::FirstRunPostDecision::DismissedUnsaved => Ok(done(
            options,
            live,
            settings_dir,
            Some("first-run:dismissed:unsaved".to_owned()),
        )),
        crate::tray::FirstRunPostDecision::DiscardPresent => {
            let (options, live) = authoritative_base(sdir, &options, cli_overrides);
            log_json_at(
                log_path,
                serde_json::json!({"event": "first-run-discarded", "reason": "file-appeared"}),
            );
            Ok(done(
                options,
                live,
                settings_dir,
                Some("first-run:discarded:present".to_owned()),
            ))
        }
        crate::tray::FirstRunPostDecision::Publish(choice) => {
            let chosen = crate::tray::settings_for_choice(choice);
            if let Err(reason) = crate::settings::validate_settings(&chosen) {
                let (options, live) = authoritative_base(sdir, &options, cli_overrides);
                log_json_at(
                    log_path,
                    serde_json::json!({"event": "first-run-discarded", "reason": reason.to_string()}),
                );
                return Ok(done(
                    options,
                    live,
                    settings_dir,
                    Some("first-run:discarded:invalid".to_owned()),
                ));
            }
            let bytes = serde_json::to_vec_pretty(&chosen)
                .map_err(|_| err("error: first-run serialize"))?;
            match crate::storage::publish_no_overwrite(
                sdir,
                crate::settings::SETTINGS_FILE_NAME,
                &bytes,
                "first-run",
            ) {
                Ok(()) => {
                    let mtime = std::fs::metadata(sdir.join(crate::settings::SETTINGS_FILE_NAME))
                        .and_then(|meta| meta.modified())
                        .ok();
                    let options = options_with_cli(&chosen, cli_overrides, &options);
                    let live = Some(LiveSettings::fresh(chosen, mtime));
                    Ok(done(
                        options,
                        live,
                        settings_dir,
                        Some(format!("first-run:{}:saved", choice.as_str())),
                    ))
                }
                Err(crate::storage::PublishError::Pending) => {
                    let (options, live) = authoritative_base(sdir, &options, cli_overrides);
                    log_json_at(
                        log_path,
                        serde_json::json!({"event": "first-run-discarded", "reason": "save-race"}),
                    );
                    Ok(done(
                        options,
                        live,
                        settings_dir,
                        Some("first-run:discarded:raced".to_owned()),
                    ))
                }
                Err(crate::storage::PublishError::Io(error)) => {
                    let mut fresh = LiveSettings::fresh(chosen.clone(), None);
                    fresh.status = format!("first-run:{}:unsaved", choice.as_str());
                    let options = options_with_cli(&chosen, cli_overrides, &options);
                    log_json_at(
                        log_path,
                        serde_json::json!({"event": "first-run-save-failed", "error": error.to_string()}),
                    );
                    Ok(done(
                        options,
                        Some(fresh),
                        settings_dir,
                        Some(format!("first-run:{}:unsaved", choice.as_str())),
                    ))
                }
            }
        }
    }
}
