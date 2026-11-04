"""Queue Cargo checks and report their exit status from the Codex Stop hook."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
QUEUE = ROOT / "target" / "cargo-stop"
CHECKS = {
    "check": ["check", "--message-format=short", "-q", "--workspace", "--tests", "-j", "1"],
    "test": ["test", "--message-format=short", "-q", "--workspace", "-j", "1"],
}


def describe(job_id):
    if not re.fullmatch(r"tmp[a-z0-9_]+", job_id):
        raise SystemExit("Invalid Cargo job ID")
    for state in ("pending", "running", "done"):
        path = QUEUE / f"{job_id}.{state}"
        if path.is_file():
            summary = path.with_suffix(".summary")
            return summary.read_text(encoding="utf-8").strip() if state == "done" and summary.is_file() else f"{job_id} {state}"
    raise SystemExit(f"Unknown Cargo job: {job_id}")


def summarize(log):
    counts = [0, 0, 0]
    error = ""
    if log.is_file():
        with log.open(encoding="utf-8", errors="replace") as output:
            for line in output:
                match = re.match(r"test result: .*?\. (\d+) passed; (\d+) failed; (\d+) ignored", line)
                if match:
                    for index, count in enumerate(match.groups()):
                        counts[index] += int(count)
                if not error and re.search(r"(^error\b|:\s*error\b| --- FAILED)", line):
                    error = line.strip()[:240]
    tests = f" | {counts[0]} passed, {counts[1]} failed, {counts[2]} ignored" if any(counts) else ""
    return tests + (f" | {error}" if error else "")


def failure_tail(log):
    with log.open("rb") as output:
        output.seek(max(0, log.stat().st_size - 2048))
        return output.read(2048).decode("utf-8", errors="replace").strip()


def run_step(command, log, env, *, quiet_after=600, deadline=1500):
    with log.open("ab", buffering=0) as output:
        size = log.stat().st_size
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT)
        started = last_output = time.monotonic()
        while process.poll() is None:
            now = time.monotonic()
            current_size = log.stat().st_size
            if current_size != size:
                size, last_output = current_size, now
            hard_timeout = now - started >= deadline
            if hard_timeout or now - last_output >= quiet_after:
                if os.name == "nt":
                    stopped = subprocess.run(
                        ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                        capture_output=True, timeout=15,
                    )
                    if stopped.returncode and process.poll() is None:
                        raise RuntimeError(f"taskkill failed for PID {process.pid}")
                else:
                    process.kill()
                process.wait(timeout=15)
                reason = f"hard timeout after {deadline:g}s" if hard_timeout else f"no output for {quiet_after:g}s"
                raise TimeoutError(reason)
            try:
                return process.wait(timeout=min(1, deadline - (now - started), quiet_after - (now - last_output)))
            except subprocess.TimeoutExpired:
                pass
        return process.returncode


def main():
    if len(sys.argv) > 1:
        checks = sys.argv[1:]
        if len(checks) == 2 and checks[0] == "status":
            print(describe(checks[1]))
            return 0
        if len(checks) > 2 or len(set(checks)) != len(checks) or any(check not in CHECKS for check in checks):
            raise SystemExit(f"Queue up to two distinct Cargo steps: {', '.join(CHECKS)}")
        QUEUE.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=QUEUE, suffix=".tmp", delete=False, mode="w") as job:
            json.dump(checks, job)
        pending = Path(job.name).with_suffix(".pending")
        Path(job.name).rename(pending)
        print(f"{pending.stem} pending")
        return 0

    event = json.loads(sys.stdin.read(64001) or "{}")
    if event.get("stop_hook_active"):
        return 0
    for pending in sorted(QUEUE.glob("*.pending")):
        running = pending.with_suffix(".running")
        try:
            pending.rename(running)
        except FileNotFoundError:
            continue
        log = running.with_suffix(".log")
        results = []
        failed = False
        try:
            env = os.environ.copy()
            rust_lib = subprocess.check_output(
                ["rustc", "--print", "target-libdir"], cwd=ROOT, text=True, timeout=30,
            ).strip()
            env["PATH"] = os.pathsep.join([
                str(ROOT / "target" / "debug" / "deps"), rust_lib, env.get("PATH", ""),
            ])
            for check in json.loads(running.read_text()):
                code = run_step(["cargo", *CHECKS[check]], log, env)
                results.append(f"{check} {'PASS' if code == 0 else f'FAIL ({code})'}")
                if code:
                    failed = True
                    break
        except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
            failed = True
            results.append(f"ERROR {str(error)[:240]}")
        summary = f"{running.stem} done | {', '.join(results)}{summarize(log)} | {log.relative_to(ROOT)}"
        running.with_suffix(".summary").write_text(summary, encoding="utf-8")
        running.rename(running.with_suffix(".done"))
        print(f"{summary}. Do not resume a paused goal.", file=sys.stderr)
        if failed and log.is_file():
            print(failure_tail(log), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
