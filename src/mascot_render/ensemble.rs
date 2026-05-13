use anyhow::{bail, Context};
use mascot_render_client::{
    mascot_render_server_address, mascot_render_server_healthcheck, mascot_render_server_status,
    set_single_character_mode_mascot_render_server, set_vpt_ensemble_mascot_render_server,
};
use mascot_render_protocol::{ServerEnsembleMode, VptEnsembleRequest};

use super::data::vpt_ensemble_character_names;
use super::logging::{
    format_mascot_json_request, format_mascot_request, log_mascot_request_result,
    report_mascot_log_failure,
};
use super::requests::post_mascot_json_request;
use super::state::vpt_ensemble_session_state;
use super::MASCOT_APPLY_TIMEOUT;

pub(super) fn configure_vpt_ensemble_startup(lines: &[String]) -> anyhow::Result<()> {
    if mascot_render_server_healthcheck().is_err() {
        return Ok(());
    }

    let status = mascot_render_server_status()?;
    configure_vpt_ensemble_startup_for_mode_with_members(
        status.ensemble_mode,
        lines,
        set_vpt_ensemble_members_mascot_render_server,
        set_vpt_ensemble_mascot_render_server,
    )
}

#[cfg(test)]
pub(super) fn configure_vpt_ensemble_startup_for_mode<F>(
    startup_mode: ServerEnsembleMode,
    lines: &[String],
    set_vpt_ensemble: F,
) -> anyhow::Result<()>
where
    F: FnOnce(&[String]) -> anyhow::Result<()>,
{
    configure_vpt_ensemble_startup_for_mode_with_members(
        startup_mode,
        lines,
        |_| Ok(()),
        set_vpt_ensemble,
    )
}

pub(super) fn configure_vpt_ensemble_startup_for_mode_with_members<M, F>(
    startup_mode: ServerEnsembleMode,
    lines: &[String],
    set_vpt_ensemble_members: M,
    set_vpt_ensemble: F,
) -> anyhow::Result<()>
where
    M: FnOnce(&[String]) -> anyhow::Result<()>,
    F: FnOnce(&[String]) -> anyhow::Result<()>,
{
    {
        let mut state = vpt_ensemble_session_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.startup_mode = Some(startup_mode);
        state.active = matches!(startup_mode, ServerEnsembleMode::Vpt);
        state.restore_single_character_on_exit = false;
    }

    let character_names = vpt_ensemble_character_names(lines);
    update_vpt_ensemble_members(&character_names, set_vpt_ensemble_members);

    if !matches!(
        startup_mode,
        ServerEnsembleMode::Favorite | ServerEnsembleMode::Vpt
    ) {
        return Ok(());
    }

    if character_names.is_empty() {
        bail!("vpt ensemble に使える mascot PSD 付き本文speakerがありません");
    }

    let address = mascot_render_server_address();
    let request_body = VptEnsembleRequest {
        character_names: character_names.clone(),
    };
    let request = format_mascot_json_request("POST", "/vpt-ensemble", address, &request_body);
    let result = set_vpt_ensemble(&character_names);
    if let Err(error) = log_mascot_request_result("vpt ensemble切替", address, &request, &result)
    {
        report_mascot_log_failure(&error);
    }
    result?;

    let mut state = vpt_ensemble_session_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.active = true;
    state.restore_single_character_on_exit = startup_mode == ServerEnsembleMode::Favorite;
    Ok(())
}

fn update_vpt_ensemble_members<F>(character_names: &[String], set_vpt_ensemble_members: F)
where
    F: FnOnce(&[String]) -> anyhow::Result<()>,
{
    let address = mascot_render_server_address();
    let request_body = VptEnsembleRequest {
        character_names: character_names.to_vec(),
    };
    let request =
        format_mascot_json_request("POST", "/vpt-ensemble/members", address, &request_body);
    let result = set_vpt_ensemble_members(character_names);
    if let Err(error) =
        log_mascot_request_result("vpt ensemble members更新", address, &request, &result)
    {
        report_mascot_log_failure(&error);
    }
    if let Err(error) = result {
        crate::runtime_notice::set_runtime_notice(format!(
            "[mascot-render] vpt ensemble members更新に失敗しました: {error}"
        ));
    }
}

pub(super) fn configure_vpt_ensemble_members(lines: &[String]) -> anyhow::Result<()> {
    if mascot_render_server_healthcheck().is_err() {
        return Ok(());
    }

    let character_names = vpt_ensemble_character_names(lines);
    update_vpt_ensemble_members(
        &character_names,
        set_vpt_ensemble_members_mascot_render_server,
    );
    Ok(())
}

fn set_vpt_ensemble_members_mascot_render_server(character_names: &[String]) -> anyhow::Result<()> {
    let body = serde_json::to_vec(&VptEnsembleRequest {
        character_names: character_names.to_vec(),
    })
    .context("failed to serialize mascot vpt ensemble members request")?;
    post_mascot_json_request(
        mascot_render_server_address(),
        "/vpt-ensemble/members",
        &body,
        MASCOT_APPLY_TIMEOUT,
    )
}

pub(super) fn restore_vpt_ensemble_session_on_exit() {
    restore_vpt_ensemble_session_on_exit_with(set_single_character_mode_mascot_render_server);
}

#[cfg(test)]
pub(super) fn restore_vpt_ensemble_session_on_exit_with<F>(set_single_character_mode: F) -> bool
where
    F: FnOnce() -> anyhow::Result<()>,
{
    let should_restore = {
        let mut state = vpt_ensemble_session_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let should_restore = state.restore_single_character_on_exit;
        state.restore_single_character_on_exit = false;
        state.active = false;
        should_restore
    };
    if !should_restore {
        return false;
    }

    let address = mascot_render_server_address();
    let request = format_mascot_request("POST", "/ensemble-mode/single-character", address, None);
    let result = set_single_character_mode();
    if let Err(error) =
        log_mascot_request_result("single character mode復元", address, &request, &result)
    {
        report_mascot_log_failure(&error);
    }
    if let Err(error) = result {
        crate::runtime_notice::set_runtime_notice(format!(
            "[mascot-render] 終了時の ensemble mode 復元に失敗しました: {error}"
        ));
    }
    true
}
