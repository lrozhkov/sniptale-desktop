use super::*;

#[test]
fn resident_agent_duplex_session_marks_tray_disconnected_only_after_controller_release()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    input.extend(framed(extension_controller_acquire())?);
    let (release_eof, wait_for_eof) = mpsc::sync_channel(1);
    let written = Arc::new(Mutex::new(Vec::new()));
    let pipe = ThreadedSplitPipe::new(input, Arc::clone(&written), wait_for_eof);
    let mut auth = pipe_auth(2, 8)?;
    let mut effects = ConnectionProjectionEffects::default();
    let events = AgentEventHub::default();

    let _ = release_eof.send(());
    run_agent_pipe_session_with_effects_events_and_state(
        pipe,
        &mut auth,
        &pipe_session_config(),
        &mut effects,
        events.subscribe(),
        NativeHostSharedState::default(),
    )?;

    assert_eq!(effects.disconnects, 1);
    assert!(effects.connected_generation.is_some());
    Ok(())
}

#[test]
fn resident_agent_duplex_session_without_controller_does_not_change_tray_connection()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    let (release_eof, wait_for_eof) = mpsc::sync_channel(1);
    let written = Arc::new(Mutex::new(Vec::new()));
    let pipe = ThreadedSplitPipe::new(input, written, wait_for_eof);
    let mut auth = pipe_auth(2, 8)?;
    let mut effects = ConnectionProjectionEffects::default();
    let events = AgentEventHub::default();

    let _ = release_eof.send(());
    run_agent_pipe_session_with_effects_events_and_state(
        pipe,
        &mut auth,
        &pipe_session_config(),
        &mut effects,
        events.subscribe(),
        NativeHostSharedState::default(),
    )?;

    assert_eq!(effects.disconnects, 0);
    Ok(())
}
