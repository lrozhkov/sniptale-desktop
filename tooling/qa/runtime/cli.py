from __future__ import annotations

import argparse
import shlex
import sys
from pathlib import Path

from .repository import ROOT, fingerprint
from .run import begin_shell_step, finish, finish_shell_step, run_step, start
from .state import assert_run_inputs, assert_state, require_harness_if_needed, write_state


def _quote(value: str) -> str:
    return shlex.quote(value)


def _start(args: argparse.Namespace) -> int:
    values = start(args.wrapper, args.mode, args.blocking, args.sensitive)
    exports = {
        "SNIPTALE_QA_RUN_ID": values["run_id"],
        "SNIPTALE_QA_ROOT_RUN_ID": values["root_run_id"],
        "SNIPTALE_QA_RUN_RECORD": values["run_path"],
        "SNIPTALE_QA_RUN_LOG": values["log_path"],
        "SNIPTALE_QA_RUN_RECORD_RELATIVE": values["run_relative"],
        "SNIPTALE_QA_RUN_LOG_RELATIVE": values["log_relative"],
        "SNIPTALE_QA_LOCK_TOKEN": values["lock_token"],
    }
    for key, value in exports.items():
        print(f"export {key}={_quote(value)}")
    return 0


def _step(args: argparse.Namespace) -> int:
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        raise SystemExit("step command is required")
    return run_step(
        Path(args.record),
        Path(args.log),
        args.step_id,
        args.label,
        command,
        args.sensitive,
        args.stream_output,
    )


def _begin_step(args: argparse.Namespace) -> int:
    started_at, started_ns = begin_shell_step(Path(args.log), args.step_id, args.label, args.sensitive)
    print(f"{started_at}|{started_ns}")
    return 0


def _finish_step(args: argparse.Namespace) -> int:
    return finish_shell_step(
        Path(args.record),
        Path(args.log),
        args.step_id,
        args.label,
        args.started_at,
        args.started_ns,
        args.exit_code,
        Path(args.output),
        args.sensitive,
    )


def _finish(args: argparse.Namespace) -> int:
    record = finish(Path(args.record), Path(args.log), args.exit_code)
    print(
        f"{record.get('wrapperId')} {record.get('status')} ({record.get('durationMs')}ms) "
        f"run={Path(args.record).relative_to(ROOT)} log={Path(args.log).relative_to(ROOT)}"
    )
    return 0


def _state(args: argparse.Namespace) -> int:
    if args.action == "write":
        write_state(ROOT, args.name, args.run_id)
    else:
        assert_state(ROOT, args.name)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="sniptale-qa-runtime")
    sub = parser.add_subparsers(dest="command", required=True)
    start_parser = sub.add_parser("start")
    start_parser.add_argument("--wrapper", required=True)
    start_parser.add_argument("--mode", default="default")
    start_parser.add_argument("--blocking", action="store_true")
    start_parser.add_argument("--sensitive", action="append", default=[])
    start_parser.set_defaults(handler=_start)

    step_parser = sub.add_parser("step")
    step_parser.add_argument("--record", required=True)
    step_parser.add_argument("--log", required=True)
    step_parser.add_argument("--step-id", required=True)
    step_parser.add_argument("--label", required=True)
    step_parser.add_argument("--sensitive", action="append", default=[])
    step_parser.add_argument("--stream-output", action="store_true")
    step_parser.add_argument("command", nargs=argparse.REMAINDER)
    step_parser.set_defaults(handler=_step)

    begin_step_parser = sub.add_parser("begin-step")
    begin_step_parser.add_argument("--log", required=True)
    begin_step_parser.add_argument("--step-id", required=True)
    begin_step_parser.add_argument("--label", required=True)
    begin_step_parser.add_argument("--sensitive", action="append", default=[])
    begin_step_parser.set_defaults(handler=_begin_step)

    finish_step_parser = sub.add_parser("finish-step")
    finish_step_parser.add_argument("--record", required=True)
    finish_step_parser.add_argument("--log", required=True)
    finish_step_parser.add_argument("--step-id", required=True)
    finish_step_parser.add_argument("--label", required=True)
    finish_step_parser.add_argument("--started-at", required=True)
    finish_step_parser.add_argument("--started-ns", required=True, type=int)
    finish_step_parser.add_argument("--exit-code", required=True, type=int)
    finish_step_parser.add_argument("--output", required=True)
    finish_step_parser.add_argument("--sensitive", action="append", default=[])
    finish_step_parser.set_defaults(handler=_finish_step)

    finish_parser = sub.add_parser("finish")
    finish_parser.add_argument("--record", required=True)
    finish_parser.add_argument("--log", required=True)
    finish_parser.add_argument("--exit-code", type=int, required=True)
    finish_parser.set_defaults(handler=_finish)

    state_parser = sub.add_parser("state")
    state_parser.add_argument("action", choices=["write", "assert"])
    state_parser.add_argument("name")
    state_parser.add_argument("--run-id", default="")
    state_parser.set_defaults(handler=_state)

    harness_parser = sub.add_parser("require-harness")
    harness_parser.set_defaults(handler=lambda _args: (require_harness_if_needed(ROOT), 0)[1])

    inputs_parser = sub.add_parser("assert-inputs")
    inputs_parser.add_argument("--record", required=True)
    inputs_parser.set_defaults(handler=lambda args: (assert_run_inputs(Path(args.record)), 0)[1])

    fingerprint_parser = sub.add_parser("fingerprint")
    fingerprint_parser.set_defaults(handler=lambda _args: (print(fingerprint()), 0)[1])
    args = parser.parse_args()
    try:
        return int(args.handler(args))
    except RuntimeError as error:
        print(f"FAIL {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
