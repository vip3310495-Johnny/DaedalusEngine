# Security policy / 資安政策

## Supported versions / 支援版本

Only the latest release receives security fixes. 只有最新版會修正資安問題。

## Reporting a vulnerability / 回報漏洞

Please report vulnerabilities privately through GitHub: **Security › Report a vulnerability** in this repository.
Do not open a public issue for a security problem.

請透過本儲存庫的 GitHub「Security › Report a vulnerability」私下回報，不要開公開的 issue。

## What the program does and does not do / 程式的行為範圍

- No network access of any kind: no update check, no telemetry. 沒有任何網路功能。
- The installer runs without administrator rights. It writes only to the chosen install folder (default
  `%LOCALAPPDATA%\Programs\DaedalusEngine`), the current user's Start menu / desktop and `HKEY_CURRENT_USER`.
  It refuses to install into a non-empty folder that is not a DaedalusEngine installation, and uninstall deletes
  only the files it installed. 安裝程式不需系統管理員權限，只寫入安裝資料夾、目前使用者的捷徑與 HKCU 登錄。
- Circuit files (`.psimx`) are data, but a **C Block** element contains C code that runs during simulation.
  It runs in the program's own interpreter with no file, network or process access, a fixed execution budget
  and bounded memory. Expression and code nesting, operator chains, buffer sizes and circuit size are limited,
  so a crafted file can at most be rejected with an error. Still, only simulate circuit files from sources you trust.
  電路檔中的 C 程式方塊會在模擬時執行，但在沙箱直譯器中執行（無檔案、網路、程序存取，有執行次數與記憶體上限）；
  仍建議只模擬可信來源的電路檔。

## Verifying a download / 驗證下載檔

Each release lists the SHA-256 of the installer in `SHA256SUMS.txt`:

```
certutil -hashfile DaedalusEngine-Setup-V<version>.exe SHA256
```

Releases built after SignPath approval are Authenticode-signed (see [CODE_SIGNING.md](CODE_SIGNING.md)).
Releases are built only by the GitHub Actions workflow in `.github/workflows/build.yml`.
