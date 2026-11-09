# Code signing policy / 程式碼簽章政策

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

## What is signed / 簽章範圍

Only files built from this repository by the GitHub Actions workflow
[`.github/workflows/build.yml`](.github/workflows/build.yml) are signed:

- `DaedalusEngine.exe` (GUI) and `daedalus-cli.exe` (command-line runner)
- `DaedalusEngine-Setup-V<version>.exe` (installer; it embeds the two signed programs above, `README.md` and `LICENSE`)

No third-party binaries are signed. Locally built files are never signed.

只有 GitHub Actions 從本儲存庫原始碼建置的檔案會被簽章；不簽第三方程式，本機建置的檔案也不簽。

## Team roles / 角色

| Role | Member |
|---|---|
| Committer and reviewer | Johnny.ch.Tsai |
| Approver (approves every signing request) | Johnny.ch.Tsai |

All team members use multi-factor authentication for GitHub and SignPath.
所有成員的 GitHub 與 SignPath 帳號皆啟用雙重驗證。

## Privacy policy / 隱私權政策

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.
DaedalusEngine has no network features: it does not check for updates, send telemetry or contact any server.

本程式不會將任何資訊傳送到其他網路系統：沒有更新檢查、沒有遙測，也不連線任何伺服器。

## Development note / 開發說明

DaedalusEngine was developed by Johnny.ch.Tsai with AI assistance (Anthropic Claude); every release is verified by the
test suites in `tools/` (theory checks of all examples, cross-validation against the original engine and ngspice).
