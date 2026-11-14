## workflow
# 用 MCP 工具建立與驗證電路（流程）

1. **釐清電路**：逐一列出元件、數值、料號與每個節點的連接；看不清楚的（極性、腳位）列為假設，交付時說明。
2. **查元件**：`daedalus_elements` 以中英文關鍵字或型別代碼搜尋，`type` 取得接腳名稱、接腳類型與參數。不要猜接腳名稱或參數名稱。
3. **建立電路**：`daedalus_new` 傳入網表（每個元件的 `pins` 寫節點名稱，接地寫 `"0"`），程式自動排版並以節點標籤連線，存成 .psimx。
   要修改已有的電路（包括使用者畫的）用 `daedalus_edit`；先用 `daedalus_inspect` 看元件與節點。
4. **檢查**：`daedalus_inspect` 列出每個節點連到哪些接腳與「問題」；問題清單必須為空（警告要逐條看懂）。
   `daedalus_render` 產生電路圖 PNG，用來目視檢查（可 highlight 某個節點或元件、pins=true 標出接腳名稱）。
5. **模擬**：`daedalus_simulate`（暫態或 AC），看每支探棒的平均、RMS、最小、最大、峰對峰與終值，需要時加量測（Freq、DutyP、Rise…）。
   數值要與手算比對（穩態值、漣波、時間常數）。回授或感測讀值接近 0、輸出遠離目標 → 多半是節點接錯或功率／控制介面不對。
6. **看波形**：`daedalus_waveform` 回傳波形圖（PNG），也可回傳降取樣的數值表。
7. **交付**：說明檔案位置、驗證數據、假設；使用者可用 `daedalus_open` 在 DaedalusEngine 開啟（程式開著時，MCP 存檔後畫面會自動重新載入）。

重點規則：
- 每個電路要有一個 SIMCTRL（`daedalus_new` 的 `sim` 會自動建立），暫態模擬至少要有一支探棒或量表（探棒名稱就是波形名稱）。
- 功率節點與控制節點不可直接相連（詳見 topic=modeling）。
- 檔案路徑一律用絕對路徑。修改使用者的檔案前先確認；`daedalus_edit` 可用 `save_as` 另存、`dry_run` 試做。

## netlist
# daedalus_new 的網表格式

```json
{
  "title": "24 V → 12 V Buck、50 kHz",
  "sim": {"dt": "0.2u", "tt": "10m"},
  "params": "Vin = 24\nfsw = 50k",
  "elements": [
    {"type": "VDC",    "name": "Vin", "params": {"V": "Vin"},           "pins": {"+": "VIN", "-": "0"}},
    {"type": "MOSFET", "name": "Q1",                                     "pins": {"D": "VIN", "S": "SW", "G": "GATE"}},
    {"type": "DIODE",  "name": "Dfw",                                    "pins": {"A": "0", "K": "SW"}},
    {"type": "L",      "name": "L1",  "params": {"L": "100u"},          "pins": {"1": "SW", "2": "LO"}},
    {"type": "IP",     "name": "IL",                                     "pins": {"in": "LO", "out": "VO"}},
    {"type": "RC",     "name": "Co",  "params": {"R": "10m", "C": "100u"}, "pins": {"1": "VO", "2": "0"}},
    {"type": "R",      "name": "RL",  "params": {"R": "6"},             "pins": {"1": "VO", "2": "0"}},
    {"type": "VP",     "name": "Vo",                                     "pins": {"n": "VO"}},
    {"type": "CONST",  "name": "Duty", "params": {"v": "0.5"},          "pins": {"out": "D"}},
    {"type": "VTRI",   "name": "Vcar", "params": {"Vpp": "1", "f": "fsw"}, "pins": {"+": "CAR", "-": "0"}},
    {"type": "COMP",   "name": "PWM",                                    "pins": {"in+": "D", "in-": "CAR", "out": "GATE"}}
  ],
  "screens": [["Vo"], ["IL"]]
}
```

- `type`：型別代碼（`daedalus_elements` 查詢）。`name`：唯一、不分大小寫；省略時自動命名。探棒與量表的名稱就是波形名稱。
- `pins`：**每一隻接腳都要寫**；值是節點名稱（同名即相連），接地寫 `"0"`，刻意不接寫 `null`。只出現一次的節點等於沒接（會提示）。
- `params`：文字或數字；未寫的用預設值。數值可用 SI 字尾（1k、4.7u、10m、1M＝百萬、1meg），可寫運算式與 `params`（參數檔）中的名稱。
- `rot`：0–3（順時針 90° 為單位，也接受 90/180/270）；`mir`：左右鏡像。
- `sim`：模擬控制參數（dt 步長、tt 總時間、tp 開始記錄時間、ps 記錄間隔步數、acc 高精度…）；省略時 dt = 10 µs、tt = 0.1 s。
- `screens`：波形檢視器的畫面（探棒名稱分組）。
- 不要放 LABEL、GND：連線全由 `pins` 的節點名稱決定，程式以導線畫出來（見下方排版）。
- 沒有接腳的元件（SIMCTRL、PARAMS、ACSWEEP、PSWEEP、MONTECARLO、DEVP、TEXT）放在最上方一列。
- 排版（以導線直連，PSIM 慣用畫法）：
  - 功率級：主電流路徑（電源 → 開關 → 電感 → 輸出）沿上方的軌由左到右，一端接地的元件（電源、電容、負載、低側開關、二極體、感測器）垂直掛在軌與下方地線之間，對地探棒接在軌上；與主路徑元件並聯的元件（反並聯二極體、緩衝電路）放在它上方。
  - 有兩個以上半橋臂（全橋、三相）時：電源正端為上方直流母線、地線在下，各臂垂直排列（上管與反並聯二極體並排），負載水平接在兩臂中點之間。
  - 控制：依訊號流向分欄（輸入在左、輸出在右），以導線相連。
  - 只有同時接到功率區與控制區的訊號（閘極、感測器輸出）用一對節點標籤；導線接不到或太遠時才以標籤代替。
  - 主路徑依網表連接推得；elements 以訊號流向排列，結果最好讀。
- 網表中的節點名稱存於檔案的 `nets` 欄位：沒有標籤的節點在 daedalus_inspect／daedalus_edit 中仍以原名（例如 SW）出現與指定。程式本身存檔後名稱不保留，屆時以「~元件.接腳」表示。

## edit
# daedalus_edit 的操作

`ops` 依序執行；任何一個失敗，整批都不存檔（錯誤訊息會指出第幾個）。每個會改變連線的操作都會以程式本身的連線規則驗證：不會讓不相干的節點接在一起、不會讓其他接腳斷線。

| op | 參數 | 說明 |
| --- | --- | --- |
| add | type, name?, params?, pins?{接腳: 節點}, x?, y?, rot?, mir?, via? | 新增元件；沒有 x、y 時放在它要連的節點附近空位；pins 的連接方式同 connect（"0" 接地，也可寫另一隻接腳如 "Q1.S"） |
| set | name, params?, rename?, rot?, mir?, x?, y?, dis? | 改參數、改名、停用；移動或旋轉時，只靠標籤連接的接腳會自動重拉，以導線直連的接腳要先 disconnect |
| remove | name, keep_wires? | 刪除元件與只服務它的導線、標籤 |
| connect | pin, net 或 to, via? | 把接腳接到節點（名稱、"0"）或另一隻接腳的節點（to: "L1.1"）：有短而不碰到其他東西的路徑時拉導線，否則用一對節點標籤；接地用接地符號。via="wire" 只接受導線、"label" 直接用標籤 |
| disconnect | pin | 拆掉這隻接腳的連線 |
| wire | from, to | 兩隻接腳間拉導線（直線、L 形或繞出）；路徑會碰到其他東西時改用 connect |
| sim | params{dt, tt, …} | 模擬控制參數 |
| params | text | 參數檔內容（每行 名稱 = 運算式） |
| screens | screens | 波形畫面，例如 [["Vo"], ["IL", "Iout"]] |
| text | text, x?, y? | 加註解文字 |

節點名稱：有標籤的節點用標籤文字，接地是 "0"，沒有標籤的節點顯示為「~元件.接腳」（例如 ~Q1.S），也可直接用來指定。
`dry_run: true` 只回報結果不存檔；`save_as` 另存新檔（原檔不變）。

## modeling
# 建模指引（精度優先）

**功率與控制**：接腳類型 功率［p］、控制輸入［ci］、控制輸出［co］、閘極［g，接控制訊號］、兩者皆可［a］。功率節點與控制節點不可直接相連：
功率 → 控制用電壓／電流感測器 `VSEN`／`ISEN`；控制 → 功率用開關閘極、`ONCTRL`、`CTOP` 或受控源。同一個控制節點只能有一個輸出驅動。

- **功率級**（Buck／Boost／Flyback／橋式／逆變器）：`MOSFET`／`IGBT`／`DIODE`（理想、分段線性，快且與 PSIM 一致）＋ `COMP`（PWM 比較器）／`GATING`／`ONCTRL` 產生閘極訊號；閘極直接接控制訊號（> 0.5 導通）。
- **類比／控制 IC 周邊**用 SPICE 物理模型：
  - `DIODE` 的 `model`：`1N4148`、`1N4007`、`1N5819`、`ds`（資料手冊參數 vfd@ifd、rd、trr 或 qrr/irr/didt、cjd@vcj、bvd）、`custom`（spice 貼 .model 參數）、`ideal`（預設）。`ZENER` 的崩潰電壓 `vb`。
  - `LED`：不模擬逆向崩潰（崩潰電壓視為無限大）。
  - `NPN3`／`PNP3`（Gummel-Poon）：2N3904、2N4401、2N2222A、2N3906；`MOS3`／`PMOS3`（level-1）。沒有確切料號時說明用了哪個近似型號。
  - 比較器 IC（LM393 類）用 `COMPOC`（開集極：V+、GND 接電源，OUT 接上拉電阻）；`TL431` 用原生 `TL431`；光耦 `OPTO1`／`OPTO2`（CTR）。
  - 開關型 `NPN`／`PNP`／`THY`／`TRIAC`／`GTO` 的基極／閘極可接控制訊號，也可接功率電路（接面視為 0.7 V 二極體）。
- **切換暫態**（米勒平台、切換損失、反向恢復）：`MOSNI`／`PMOSNI`／`IGBTNI`（閘極接控制訊號，內含驅動器 von/voff 與 rg），dt ≤ Rg·Ciss/5（通常 ≤ 1 ns）；`DEVP`（不接線，el＝開關名稱，q＝vg/vd/id/ig/p/e）。
- **量測**：`VP`（對地電壓）、`VP2`（兩點電壓）、`IP`（電流，in→out）；`VDCM`／`VACM`／`ADCM`／`AACM` 直流／交流表；`WATT` 瓦特計（電流 I+→I−，電壓 I+ 對 V−）；`WH3` 三相瓦時計。
- **固定參考值**用 `CONST`（控制訊號），不要用功率電壓源接控制節點。控制電路用的電源（VSIN、VSQU、VTRI…）負端接地。MCU 方波用 `VSQU` 或 `VPWL`。
- **參數檔** `PARAMS`；**參數掃描** `PSWEEP`（var、vals）；**蒙地卡羅** `MONTECARLO`（vars 每行 `R1.R 5% gauss`）。
- **AC 掃頻**：`ACSWEEP`（src＝激勵源名稱；method 0＝小訊號注入法，可分析開關電路；1＝線性頻域法）＋ `ACP` 探棒（迴路增益用 `ACP2`）；激勵源用 `VSIN` 且 Vm=0。用 `daedalus_simulate` 的 analysis="ac"。
- **時間步長**：比最快的時間常數或邊緣小 10 倍以上；開關電源取開關週期的 1/100 以下（50 kHz → dt ≤ 0.2 µs）；SPICE 類比電路 1 µs 常見。警告「細分到上限仍超過容許值」代表 dt 太大。
- **控制演算法**：`PI`、`LIM`、`SUB2`、濾波器、`TFS`、`MATH1`～`MATH10`（運算式）、`CBLOCK`（簡化 C，nin／nout 決定接腳 x1…、y1…）；數位控制 `ZOH`、`UDELAY`、`FILTD`／`TFZ`、`FIR`。代數迴路用 `UTD` 斷開；除法加下限（max(x, 1e-3)）。

已知注意事項：
- 電容直接並聯電壓源時初值不一致會警告；輸入電容用 `RC`（串聯 ESR 10–50 mΩ）較穩。
- `VCCVS`／`ICCCS` 的控制端必須跨在一個 R／L／C 支路兩端；沒有時用串接式 `VCCVS1`／`ICCCS1`。
- 隔離的次級側用大電阻（10 MΩ）參考到初級地，避免浮接。`TF3YD` 二次側 Δ 迴路給一點 ls／rs。
- 理想開關關斷電阻預設 1 MΩ；高阻抗類比電路把 roff 改大（1G 以上）。
- t = 0：電感為電流源（i0）、電容為電壓源（v0）；給接近實際的初值可縮短啟動暫態。
- 三相：`VSIN3` 的 V 是線電壓有效值；三線制電網中性點不要直接接地。

## measure
# 量測（daedalus_simulate 的 measures）

預設統計：平均（Mean）、RMS、最小、最大、峰對峰、終值，統計區間預設為最後 20 %（`from`、`to` 可指定時間，例如 "8m"）。
可加的量測（名稱不分大小寫）：

| 名稱 | 定義 |
| --- | --- |
| Max、Min、Pp | 最大、最小、峰對峰 |
| Top、Base、Amp | 高準位、低準位（直方圖最常出現的準位；沒有平頂時＝Max/Min）、振幅 Top−Base |
| Mean、Rms、AcRms | 時間平均、均方根、去除直流的均方根 |
| CycMean、CycRms | 整數個週期的平均、均方根（不受範圍邊緣影響，穩態漣波用這個） |
| Crest | 峰值因數 max(|Max|,|Min|)/RMS |
| OverP、OverN | 正／負過衝（% 振幅） |
| Area | ∫v dt |
| Freq、Period | 頻率、週期（50 % 準位、±10 % 遲滯的上升緣） |
| WidthP、WidthN、DutyP、DutyN | 正／負脈寬、正／負工作週期（%） |
| Rise、Fall | 10 %→90 % 上升時間、90 %→10 % 下降時間 |

AC 掃頻：每條曲線列出低頻增益、峰值、−3 dB 頻寬；迴路增益（ACP2）列出穿越頻率、相位裕度 PM 與增益裕度 GM，另附對數間隔的增益／相位表。

## troubleshoot
# 常見問題

- **「功率電路與控制電路直接相連」**：一個節點同時接了功率接腳與控制接腳。功率訊號送進控制用 VSEN／ISEN；控制訊號驅動功率用開關閘極、ONCTRL、CTOP。
- **「X 沒有連接」**：該接腳的節點只有它自己。檢查節點名稱拼字；只出現一次的節點名稱等於沒接。
- **輸出為 0 或遠離預期**：用 `daedalus_inspect` 核對節點；用 `daedalus_render` 的 highlight 看該節點；確認開關閘極有訊號（加 VP 在控制節點上）、PWM 載波與比較器輸入方向。
- **「細分到上限仍超過容許值」「開關未收斂」**：dt 太大或電路有不合理的理想元件組合（理想電壓源並聯電容、理想開關串電感無續流路徑）；縮小 dt、加續流二極體或 ESR。
- **模擬太久**：先用 `tt` 縮短總時間試跑；`timeout` 可放寬上限（預設 120 秒）。
- **NaN**：除以零（控制運算啟動時為 0）、開根號負數；加下限或改用 LIM。
