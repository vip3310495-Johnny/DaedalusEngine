# 發布與 SignPath 免費簽章設定步驟

只需做一次。帶 ★ 的步驟需要你本人操作（登入、身分與核准都綁在你的帳號上）。

## 1. GitHub 儲存庫 ★

1. 在 GitHub 帳號啟用雙重驗證（Settings › Password and authentication）。
2. 建立**公開**儲存庫 `DaedalusEngine`（不要勾選自動產生 README／LICENSE，本機已有）。
3. 告訴 Claude 儲存庫網址，由 Claude 完成首次推送（推送前會再給你確認一次）。
4. 推送後到 Actions 分頁確認 `build` 工作流程通過（建置、單元測試、32 個範例的理論驗證）。

## 2. 第一次發布（未簽章）

SignPath 要求專案「已以要簽章的形式發布過」。推送標籤 `v1.1.0` 後，工作流程會自動建立 GitHub Release，
附上安裝程式與 SHA-256（此時尚未簽章）。

## 3. 申請 SignPath Foundation ★

1. 到 <https://signpath.org/apply> 填寫申請：專案名稱 DaedalusEngine、儲存庫網址、授權 GPL-3.0、
   程式碼簽章政策連結（儲存庫中的 `CODE_SIGNING.md`）。
2. 等待人工審核（時間不一定；可能被要求補充資料，也可能不通過）。
3. 通過後依 SignPath 的指示在 SignPath 建立帳號並啟用雙重驗證。

## 4. 在 SignPath 建立設定 ★（核准後）

在 SignPath 的專案 `DaedalusEngine` 中：

- 簽章政策（signing policy）slug：`release-signing`，核准者設為你自己。
- 兩個 artifact configuration：

`programs`（程式本體，GitHub artifact 是 zip 檔）：

```xml
<?xml version="1.0" encoding="utf-8"?>
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="DaedalusEngine.exe"><authenticode-sign/></pe-file>
    <pe-file path="daedalus-cli.exe"><authenticode-sign/></pe-file>
  </zip-file>
</artifact-configuration>
```

`installer`（安裝程式）：

```xml
<?xml version="1.0" encoding="utf-8"?>
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="DaedalusEngine-Setup-V*.exe"><authenticode-sign/></pe-file>
  </zip-file>
</artifact-configuration>
```

- 連結 GitHub 儲存庫為信任的建置來源（Trusted Build System：GitHub.com），讓 SignPath 驗證檔案確實由本儲存庫的工作流程建置。

## 5. 把金鑰交給 GitHub ★

在 GitHub 儲存庫 Settings › Secrets and variables › Actions：

- **Secret** `SIGNPATH_API_TOKEN`：SignPath 給 CI 用的 API token（只貼在這裡，不要貼在對話或檔案中）。
- **Variable** `SIGNPATH_ORGANIZATION_ID`：SignPath 組織 ID。

## 6. 之後每次發布

1. 依版本規則改版號（Claude 會處理），推送標籤 `v<版本>`。
2. 工作流程建置、測試後送出兩次簽章請求（程式本體、安裝程式）；★ 你在 SignPath 按「核准」。
3. 完成後 GitHub Release 自動附上已簽章的安裝程式與 SHA-256。

注意：SignPath 的實際設定畫面與欄位名稱以其官方文件為準；本文的 XML 與 slug 名稱需與 `.github/workflows/build.yml` 一致。
