from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from config import LabConfig
from errors import IntegrityError, LabError
from paths import LabPaths
from state import write_json
from vm import RunningVm, VmManager
from runner import LaneRunner


class VmTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "repo"
        config_dir = self.root / "tooling/desktop/e2e_lab"
        config_dir.mkdir(parents=True)
        source = Path(__file__).resolve().parents[1] / "lab-config.json"
        (config_dir / "lab-config.json").write_bytes(source.read_bytes())
        self.paths = LabPaths.discover(self.root)
        self.paths.ensure()
        self.config = LabConfig.load(self.root)
        self.manager = VmManager(self.paths, self.config)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_stale_pid_state_is_removed(self) -> None:
        runtime = self.paths.runtime / "linux-x11"
        runtime.mkdir(parents=True)
        write_json(runtime / "state.json", {
            "lane": "linux-x11", "pid": 999999, "overlay": str(runtime / "overlay.qcow2"),
            "qmp_socket": str(runtime / "qmp.sock"), "started_epoch": 1,
            "golden_sha256": "a" * 64, "guest_network": False,
            "windows_audio_capture": False,
        })
        self.assertIsNone(self.manager.current("linux-x11"))
        self.assertFalse(runtime.exists())

    def test_corrupted_snapshot_identity_fails_closed(self) -> None:
        golden = self.paths.golden / "linux-x11"
        golden.mkdir(parents=True)
        (golden / "disk.qcow2").write_bytes(b"disk")
        write_json(golden / "identity.json", {"disk_sha256": "0" * 64})
        with mock.patch("vm.wait_for_resources", return_value={}):
            with self.assertRaisesRegex(IntegrityError, "identity mismatch"):
                self.manager.start("linux-x11")

    def test_test_phase_can_use_smaller_explicit_host_memory_reserve(self) -> None:
        with mock.patch("vm.wait_for_resources", return_value={}) as resources:
            with self.assertRaisesRegex(LabError, "golden image is missing"):
                self.manager.start("windows-11", host_reserve_mib=1024)
        self.assertEqual(resources.call_args.kwargs["min_memory_mib"], 8192 + 1024)
        with self.assertRaisesRegex(IntegrityError, "host memory reserve"):
            self.manager.start("windows-11", host_reserve_mib=512)

    def test_reset_recovers_corrupted_runtime_state_without_broad_process_kill(self) -> None:
        runtime = self.paths.runtime / "linux-x11"
        runtime.mkdir(parents=True)
        (runtime / "state.json").write_text("{}", encoding="utf-8")
        with mock.patch.object(self.manager, "orphan_pids", return_value=[]):
            self.assertFalse(self.manager.reset("linux-x11"))
        self.assertFalse(runtime.exists())

    def test_runtime_lane_symlink_is_rejected(self) -> None:
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        (self.paths.runtime / "linux-x11").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(IntegrityError, "runtime directory is redirected"):
            self.manager.current("linux-x11")

    def test_fresh_runtime_removes_state_less_symlink_entries_without_following(self) -> None:
        outside = Path(self.temp.name) / "outside.log"
        outside.write_text("preserve", encoding="utf-8")
        runtime = self.paths.runtime / "linux-x11"
        runtime.mkdir()
        (runtime / "qemu.log").symlink_to(outside)
        fresh = self.manager._fresh_runtime("linux-x11")
        self.assertEqual(list(fresh.iterdir()), [])
        self.assertEqual(outside.read_text(encoding="utf-8"), "preserve")

    def test_forced_shutdown_is_reported_and_runtime_removed(self) -> None:
        runtime = self.paths.runtime / "linux-x11"
        runtime.mkdir(parents=True)
        state = {
            "lane": "linux-x11", "pid": 2222, "overlay": str(runtime / "overlay.qcow2"),
            "qmp_socket": str(runtime / "qmp.sock"), "started_epoch": 1,
            "golden_sha256": "a" * 64, "guest_network": False,
            "windows_audio_capture": False,
        }
        write_json(runtime / "state.json", state)
        vm = RunningVm("linux-x11", 2222, Path(state["overlay"]), Path(state["qmp_socket"]), 1, "a" * 64, False)
        with mock.patch.object(self.manager, "current", return_value=vm), \
             mock.patch.object(self.manager, "qmp"), \
             mock.patch("vm.process_matches", side_effect=[True, True]), \
             mock.patch("vm.terminate_process", return_value=True) as terminate:
            with mock.patch("vm.time.monotonic", side_effect=[0, 999999]):
                forced = self.manager.stop("linux-x11")
        self.assertTrue(forced)
        terminate.assert_called_once_with(2222, timeout=5)
        self.assertFalse(runtime.exists())

    def test_qmp_timeout_is_a_recoverable_lab_error(self) -> None:
        vm = RunningVm(
            "linux-x11", 2222, Path("/overlay.qcow2"), Path("/qmp.sock"),
            1, "a" * 64, False,
        )
        client = mock.MagicMock()
        client.__enter__.return_value = client
        client.recv.side_effect = TimeoutError("stopped guest")
        with mock.patch.object(Path, "exists", return_value=True), \
             mock.patch("vm.socket.socket", return_value=client):
            with self.assertRaisesRegex(LabError, "QMP transport failed"):
                self.manager.qmp(vm, "system_powerdown")

    def test_qmp_supports_bounded_command_arguments(self) -> None:
        vm = RunningVm(
            "windows-11", 2222, Path("/overlay.qcow2"), Path("/qmp.sock"),
            1, "a" * 64, True,
        )
        client = mock.MagicMock()
        client.__enter__.return_value = client
        client.recv.side_effect = [b'{"QMP":{}}\r\n', b'{"return":{}}\r\n', b'{"return":{}}\r\n']
        with mock.patch.object(Path, "exists", return_value=True), \
             mock.patch("vm.socket.socket", return_value=client):
            self.manager.qmp(
                vm, "send-key", {"keys": [{"type": "qcode", "data": "spc"}]},
            )
        request = json.loads(client.sendall.call_args_list[-1].args[0])
        self.assertEqual(request["execute"], "send-key")
        self.assertEqual(request["arguments"]["keys"][0]["data"], "spc")

    def test_qemu_command_binds_management_only_to_loopback(self) -> None:
        lane = self.config.lanes["linux-wayland"]
        with mock.patch.object(self.manager, "_qemu", return_value="qemu-system-x86_64"):
            command = self.manager._command(lane, Path("/lab/overlay.qcow2"), Path("/lab/qmp.sock"), Path("/lab"))
        joined = " ".join(command)
        self.assertIn("-no-reboot", command)
        self.assertIn(f"hostfwd=tcp:127.0.0.1:{lane.ssh_port}-:22", joined)
        self.assertIn(f"127.0.0.1:{lane.vnc_display}", joined)
        self.assertNotIn("0.0.0.0", joined)
        self.assertIn("restrict=on", joined)
        self.assertIn("virtio-net-pci,netdev=net0,mac=52:54:00:12:34:56,addr=0x1", joined)
        self.assertIn("virtio-blk-pci,drive=system,addr=0x2", joined)
        with mock.patch.object(self.manager, "_qemu", return_value="qemu-system-x86_64"):
            build_command = self.manager._command(
                lane, Path("/lab/overlay.qcow2"), Path("/lab/qmp.sock"), Path("/lab"),
                allow_guest_network=True,
            )
        self.assertIn("restrict=off", " ".join(build_command))

    def test_linux_runtime_lockdown_sets_output_drop(self) -> None:
        script = Path(__file__).resolve().parents[1] / "guest/linux-network-lockdown.sh"
        value = script.read_text(encoding="utf-8")
        self.assertIn("iptables -P OUTPUT DROP", value)
        self.assertIn("/run/sniptale-e2e-network-policy", value)

    def test_runner_resets_after_shutdown_state_read_failure(self) -> None:
        runner = LaneRunner(self.paths, self.config)
        with mock.patch.object(runner, "_request_guest_shutdown", side_effect=IntegrityError("corrupt state")), \
             mock.patch.object(runner.vms, "reset", return_value=False) as reset:
            with self.assertRaisesRegex(Exception, "reset completed"):
                runner._finish_vm("linux-wayland", "test", False)
        reset.assert_called_once_with("linux-wayland")
        self.assertFalse(runner.teardowns["linux-wayland"]["test_post_forced"])

    def test_acceptance_test_fails_closed_without_kvm_before_vm_mutation(self) -> None:
        runner = LaneRunner(self.paths, self.config)
        with mock.patch("runner.os.access", return_value=False), \
             mock.patch.object(runner, "_test_linux") as test_linux:
            with self.assertRaisesRegex(LabError, "acceptance requires"):
                runner.test("linux-wayland", Path("artifact.deb"), self.paths.lab / "run")
        test_linux.assert_not_called()

    def test_windows_companion_preflight_runs_before_vm_reset(self) -> None:
        runner = LaneRunner(self.paths, self.config)
        artifact = self.root / "SniptaleNativeAgent-dev-0.1.0-dev.msi"
        artifact.write_bytes(b"msi")
        with mock.patch.object(runner.vms, "reset") as reset:
            with self.assertRaisesRegex(LabError, "baseline MSI"):
                runner._test_windows("windows-11", artifact, self.paths.lab / "run")
        reset.assert_not_called()

    def test_windows_runtime_uses_writable_firmware_copy(self) -> None:
        lane = self.config.lanes["windows-11"]
        golden = self.paths.golden / lane.name
        runtime = self.paths.runtime / lane.name
        golden.mkdir()
        runtime.mkdir()
        for name in ("OVMF_CODE.fd", "OVMF_VARS.fd"):
            path = golden / name
            path.write_bytes(b"firmware")
            path.chmod(0o444)
        with mock.patch.object(self.manager, "_qemu", return_value="qemu-system-x86_64"):
            command = self.manager._command(lane, runtime / "overlay.qcow2", runtime / "qmp.sock", runtime)
        self.assertEqual((runtime / "OVMF_VARS.fd").stat().st_mode & 0o777, 0o600)
        self.assertEqual((golden / "OVMF_VARS.fd").stat().st_mode & 0o777, 0o444)
        self.assertIn("e1000e,netdev=net0,mac=52:54:00:12:34:56,addr=0x1", " ".join(command))
        self.assertNotIn("-no-reboot", command)
        joined = " ".join(command)
        self.assertIn("driver=wav,id=audio0,path=", joined)
        self.assertIn("system-audio-output.wav", joined)
        self.assertIn("hda-duplex,audiodev=audio0", joined)

        microphone_runtime = self.paths.runtime / "windows-microphone"
        microphone_runtime.mkdir()
        with mock.patch.object(self.manager, "_qemu", return_value="qemu-system-x86_64"):
            microphone_command = self.manager._command(
                lane,
                microphone_runtime / "overlay.qcow2",
                microphone_runtime / "qmp.sock",
                microphone_runtime,
                windows_audio_capture=True,
            )
        microphone_joined = " ".join(microphone_command)
        self.assertIn("driver=alsa,id=audio0", microphone_joined)
        self.assertIn("in.dev=sniptale_e2e_mic", microphone_joined)
        self.assertIn("out.dev=sniptale_e2e_speaker", microphone_joined)

    def test_windows_runtime_uses_unbounded_ephemeral_microphone_fixture(self) -> None:
        runtime = self.paths.runtime / "windows-audio"
        runtime.mkdir()
        config = self.manager._prepare_windows_microphone(runtime)
        config_text = config.read_text(encoding="utf-8")
        self.assertIn('infile "/dev/urandom"', config_text)
        self.assertIn("pcm.sniptale_e2e_speaker", config_text)
        self.assertIn("system-audio-output-s16le.raw", config_text)
        self.assertFalse(any(runtime.glob("*.raw")))

    def test_windows_qemu_log_is_bounded_redacted_evidence(self) -> None:
        runtime = self.paths.runtime / "windows-11"
        runtime.mkdir()
        (runtime / "qemu.log").write_text(
            "backend failed under /home/private-user/lab token=private-value\n",
            encoding="utf-8",
        )
        run_dir = self.paths.lab / "run"
        (run_dir / "logs").mkdir(parents=True)
        runner = LaneRunner(self.paths, self.config)

        runner._collect_qemu_log("windows-11", run_dir)

        evidence = run_dir / "logs/windows-11-qemu.log"
        value = evidence.read_text(encoding="utf-8")
        self.assertNotIn("private-user", value)
        self.assertNotIn("private-value", value)
        self.assertEqual(evidence.stat().st_mode & 0o777, 0o600)

    def test_windows_audio_backend_guard_rejects_qemu_voice_failures(self) -> None:
        runtime = self.paths.runtime / "windows-11"
        runtime.mkdir()
        log = runtime / "qemu.log"
        runner = LaneRunner(self.paths, self.config)
        log.write_text("", encoding="utf-8")
        runner._assert_windows_audio_backend("windows-11")
        log.write_text(
            "alsa: Could not initialize ADC\n"
            "audio: Could not create a backend for voice `adc'\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(LabError, "QEMU audio backend is unhealthy"):
            runner._assert_windows_audio_backend("windows-11")
        runner._assert_windows_audio_backend("windows-11", require_capture=False)
        log.write_text(
            "audio: Could not create a backend for voice `dac'\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(LabError, "QEMU audio backend is unhealthy"):
            runner._assert_windows_audio_backend(
                "windows-11",
                require_capture=False,
            )


if __name__ == "__main__":
    unittest.main()
