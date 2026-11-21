# Provider fixtures

Real JSONL output from each installed AI CLI, recorded by `scripts/capture-fixtures.sh`. Parser tests replay these files.

## How to run

Run from the repository root:

```bash
bash scripts/capture-fixtures.sh --list
bash scripts/capture-fixtures.sh
```

`--list` prints every command without running it. A real run needs the CLIs on `PATH`. The image cases use the committed synthetic `tiny.png` (64x64), so no screenshot or Screen Recording permission is involved. A CLI that is not on `PATH` is skipped with a message. The script never passes a permission-bypass flag.

## Review before committing

Fixtures can contain your account email, organization names and session ids. Read every file in the case directories before you commit them.

## Layout

Each CLI has its own directory: `<id>/`, where `<id>` is `claude`, `codex`, `agy` or `gemini`.

- `<case>.jsonl`: stdout of the command.
- `<case>.stderr.txt`: stderr of the command.
- `<case>.exit.txt`: exit code of the command.
- `VERSION`: output of `<id> --version` at capture time.
- `codex/resume.variant.txt`: which `codex exec resume` argument order succeeded (`exec-flags-then-resume`, `resume-then-flags`, or `none`).

Cases:

| CLI | Cases |
| --- | --- |
| claude | `text`, `resume`, `image`, `denied`, `error` |
| codex | `text`, `resume`, `image`, `mcp_override`, `error` |
| agy | `text`, `resume`, `image`, `denied`, `mcp_workspace` |
| gemini | `text`, `resume`, `error` |

If a case cannot run because its input is missing (for example, no session id in the text case), the script writes the reason to `<case>.stderr.txt` and does not write an exit file.

The `agy` and `gemini` flags are not verified yet, because neither CLI was installed when this script was written. Check them on a real run and record the results in `docs/RESEARCH.md`.
