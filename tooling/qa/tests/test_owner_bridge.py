"""The Windows worker must retain event and lifecycle-log forwarding after borrowing inputs."""

from pathlib import Path
import unittest
import unittest.mock as mock

from tooling.qa.rules import verify_architecture as rules


class OwnerBridgeTests(unittest.TestCase):
    def test_event_and_log_bridge_rejects_disconnected_inputs(self):
        source = (Path(__file__).resolve().parents[3] / "src/app/resident/pipe.rs").read_text()
        with (
            mock.patch.object(rules, "read", return_value=source),
            mock.patch.object(rules, "fail") as fail,
        ):
            rules.check_windows_pipe_worker_event_bridge()
            fail.assert_not_called()
        for original, replacement in (
            (
                "&agent_events,\n            log.as_ref()",
                "&AgentEventHub::default(),\n            log.as_ref()",
            ),
            (
                "&agent_events,\n            log.as_ref()",
                "&agent_events,\n            None",
            ),
            (
                "effects_factory, agent_events, log,",
                "effects_factory, agent_events, None,",
            ),
            ("run_with_effects_events_and_log", "run_with_effects"),
        ):
            with self.subTest(replacement=replacement):
                changed = source.replace(original, replacement)
                self.assertNotEqual(changed, source)
                with (
                    mock.patch.object(rules, "read", return_value=changed),
                    mock.patch.object(rules, "fail") as fail,
                ):
                    rules.check_windows_pipe_worker_event_bridge()
                    self.assertTrue(fail.called)

    def test_pipe_instance_budget_requires_the_same_checked_session_limit(self):
        root = Path(__file__).resolve().parents[3]
        adapter = (root / "src/ipc/pipe/windows_pipe.rs").read_text()
        source = (root / "src/app/resident/pipe_windows.rs").read_text()

        def check(text):
            with (
                mock.patch.object(rules, "read", side_effect=[adapter, text]),
                mock.patch.object(rules, "fail") as fail,
            ):
                rules.check_windows_pipe_allows_concurrent_instances()
                return fail.called

        self.assertFalse(check(source))
        for replacement in ("1", "u32::MAX", "PIPE_MAX_SESSIONS as u32"):
            changed = source.replace(
                "u32::try_from(PIPE_MAX_SESSIONS).map_err(|_| AgentPipeServerError::Accept)?",
                replacement,
            )
            self.assertNotEqual(changed, source)
            self.assertTrue(check(changed))
