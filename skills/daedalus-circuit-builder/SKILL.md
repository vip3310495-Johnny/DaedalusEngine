---
name: "daedalus-circuit-builder"
description: "Build a .psimx circuit file for DaedalusEngine (Windows power-electronics simulator, PSIM-style) from a schematic, photo or description, check its connectivity and layout, and verify it by simulating it with DaedalusEngine's command-line simulator."
---

# 建立 DaedalusEngine 電路檔（.psimx）

用途：把使用者給的電路（Altium／PSIM 截圖、手繪、文字描述、規格）轉成 DaedalusEngine 可直接「檔案 › 開啟」或拖曳載入的 `.psimx`，交付前**用程式本身模擬一次**確認可模擬、數值合理，**且圖面可讀**（分區、區內直連、跨區介面清楚）。`.psimx` 與網頁版「電力電子電路模擬器」通用。

> **有 MCP 時優先用 MCP。** 環境若有 DaedalusEngine 的 MCP 工具（`daedalus_new`、`daedalus_edit`、`daedalus_inspect`、`daedalus_simulate`、`daedalus_render`…），直接用它們建立、修改、檢查與模擬電路：網表以節點名稱連線、修改經程式的連線規則驗證、可看電路圖與波形圖，不需要 Python。先讀 `daedalus_guide`（topic=workflow、modeling）。設定方式（Claude Code）：
> `claude mcp add daedalus -- "%LOCALAPPDATA%\Programs\DaedalusEngine\daedalus-cli.exe" mcp`（程式內「說明 › AI 與 MCP」可複製）。
> **沒有 MCP 時，優先用 daedalus-cli 由網表自動排版**（與 MCP 相同的排版器，以導線直連、只有跨功率／控制的訊號用標籤）：
> 1. 讀網表格式：`daedalus-cli tool daedalus_guide "{\"topic\":\"netlist\"}"`；查元件：`daedalus-cli tool daedalus_elements "{\"type\":\"MOSFET\"}"`。
> 2. 寫網表 JSON（每隻接腳寫節點名稱，接地 `"0"`）→ `daedalus-cli new netlist.json circuit.psimx`（輸出節點表與問題清單）。
> 3. `daedalus-cli render circuit.psimx circuit.png --pins` 產生電路圖（PNG 或 SVG，不需要開程式）並實際看圖；`daedalus-cli inspect circuit.psimx` 看節點。
> 4. `scripts/verify_psimx.py` 或 `daedalus-cli tool daedalus_simulate "{\"path\":\"…\"}"` 模擬驗證。
> 需要手工控制每個座標時才用以下 Python 腳本；第 3、4 節的建模與版面規則都適用。

本資料夾：

| 檔案 | 用途 |
| --- | --- |
| `scripts/psimx_builder.py` | Python 產生器 `Doc`（放元件、取接腳座標、畫導線、靜態檢查、存檔） |
| `scripts/elements.json` | 全部元件的接腳偏移與參數預設值（由程式的元件庫自動產生，與程式一致） |
| `scripts/verify_psimx.py` | 靜態檢查＋用 `daedalus-cli` 模擬＋每支探棒的統計，可加 `--expect` 判定 |
| `scripts/shot_psimx.py` | 用 DaedalusEngine 開檔截圖（目視檢查版面），可加 `--run` 連波形一起截 |
| `references/elements.md` | 元件參考：型別代碼、中文名、接腳（座標、類型）、參數預設值、說明 |
| `examples/buck_converter.py` | 完整範例：分區、軌、介面標籤、探棒（CI 每次建置都會實際模擬驗證） |

## 1. 檔案格式

`.psimx` 是 UTF-8 JSON：

```json
{ "app": "psimweb", "version": "V1.5.1",
  "doc": { "elements": [ ... ], "wires": [ ... ], "nid": 123 },
  "screens": [["Vo"], ["IL", "Iout"]] }
```

- element：`{"id":1,"type":"R","x":4,"y":2,"rot":0,"mir":false,"dis":false,"name":"R1","params":{"R":"1k"}}`
  - `type`：型別代碼（`references/elements.md`）。**不認得的型別會被程式默默丟掉**，`check()` 會列為錯誤。
  - `name`：需唯一且**不分大小寫**（`Vs` 與 `vs` 衝突）；GND／LABEL／TEXT／SIMCTRL 等用空字串。探棒與量表的名稱就是波形名稱；AC 掃頻以名稱指定激勵源；元件內部探棒 `DEVP` 以名稱指定元件。
  - `params`：全部用字串，未寫的參數用預設值。數值可用 SI 字尾（`1k`、`4.7u`、`10m`、`1M`＝百萬、`1meg`）、單位字母會被忽略（`10mH`）、可寫運算式與參數檔名稱（見說明書「運算式語法」）。
  - `rot`：0–3（每次順時針 90°）；`mir`：左右鏡像（先鏡像再旋轉）；`dis`：停用（不參與模擬）。
- wire：`{"id":2,"x1":4,"y1":2,"x2":8,"y2":2}`，必須水平或垂直。
- `nid`：大於所有 id。座標都是整數格點，y 向下為正。
- 選擇性 `screens`：波形檢視器的畫面配置（探棒名稱分組）。

## 2. 連線規則（與程式完全相同）

- 接腳世界座標 = 元件 (x,y) + 接腳偏移（`elements.json`，rot=0）：先 `mir` 則 x → −x；再依 rot：0:(x,y)、1:(−y,x)、2:(−x,−y)、3:(y,−x)。**一律用 `d.pin(el, '腳名')` 取座標，不要手算。**
- 座標相同的點相連（接腳–接腳、接腳–導線端點、導線端點–導線端點）。
- 一個點落在另一條導線的**內部**也相連（T 接點）。導線交叉但沒有端點落在對方上 → 不相連。**所以不相干的接腳或導線端點不可剛好落在別的導線上。**
- 同名 `LABEL`（`params.text`）彼此相連；`GND` 為 0 V。**只放一個的 LABEL 不會報錯，但等於沒接**——`check()` 的 `single_labels` 必須為空。
- 接腳類型（`elements.json` 第 4 欄）：`P` 功率、`Ci`／`Co` 控制輸入／輸出、`G` 閘極（接控制訊號）、`A` 兩者皆可。**功率節點與控制節點不可直接相連**：功率→控制用 `VSEN`／`ISEN`，控制→功率用開關閘極、`ONCTRL`、`CTOP` 或受控源。同一個控制節點只能有一個輸出驅動。
- 只連到一個東西的接腳是開路（`open_pins`，程式也會標示）。每個電路必須有一個 `SIMCTRL`；暫態模擬至少要有一支探棒或量表。
- 程式中滑鼠停在接腳上會顯示接腳名稱與用途；接腳旁的標記 ■ 功率、▶ 控制（指向元件＝輸入）、◇ 皆可，截圖時可用來核對。

## 3. 建模指引（精度優先）

- **功率級**（Buck／Boost／Flyback／橋式／逆變器）：`MOSFET`／`IGBT`／`DIODE`（理想、分段線性，快且與 PSIM 一致）＋ `COMP`（控制比較器，PWM）／`GATING`／`ONCTRL` 產生閘極訊號；閘極直接接控制訊號（> 0.5 為導通）。
- **類比／控制 IC 周邊**用 SPICE 物理模型：
  - `DIODE`：`model` = `1N4148`、`1N4007`、`1N5819`、`ds`（資料手冊參數 `vfd`@`ifd`、`rd`、`trr` 或 `qrr`/`irr`/`didt`、`cjd`@`vcj`、`bvd`）、`custom`（`spice` 貼 .model 參數）或 `ideal`（預設）。`ZENER`：`model=ZENER`，崩潰電壓取 `vb`。
  - `LED`：`model` = `ideal`、`LEDRED`、`custom`。**發光二極體不模擬逆向崩潰**（假設不會損壞，崩潰電壓視為無限大，`spice` 中的 BV 會被忽略）。
  - `NPN3`／`PNP3`（Gummel-Poon）：`2N3904`（預設）、`2N4401`、`2N2222A`、`2N3906`（PNP 預設）、`custom`、`3state`。`MOS3`／`PMOS3`（level-1）：`GENERIC_N`、`GENERIC_P`、`custom`、`3state`。沒有確切料號時說明用了哪個近似型號。
  - 比較器 IC（LM393／LM339／LM2903 類）用 `COMPOC`（開集極已內建：V+、GND 接電源，OUT 直接接上拉電阻）；不要用 `COMP`＋開關拼湊。`TL431` 用原生 `TL431`，光耦用 `OPTO1`／`OPTO2`（CTR 參數）。
  - 開關型 `NPN`／`PNP`／`THY`／`TRIAC`／`GTO` 的基極／閘極可接控制訊號，也可直接接功率電路（接面視為 0.7 V 二極體，汲取基極／閘極電流）。
- **切換暫態**（米勒平台、切換損失、反向恢復、IGBT 拖尾）用 `MOSNI`／`PMOSNI`／`IGBTNI`：閘極接控制訊號（內含驅動器 `von`／`voff` 與 `rg`），參數照資料手冊。dt 要 ≤ Rg·Ciss/5（通常 ≤ 1 ns）。用 `DEVP`（不接線，`el`＝開關名稱，`q`＝vg/vd/id/ig/p/e）看閘極電壓、功率與累積能量（游標夾住一次切換，`e` 的差＝Eon/Eoff）。整機長時間模擬仍用理想開關。
- **量測**：`VP`（對地）、`VP2`（兩點）、`IP`（電流，in→out）；`VDCM`／`VACM`／`ADCM`／`AACM` 直流／交流表；`WATT` 瓦特計（電流 I+→I−，電壓 I+ 對 V−）；`WH3` 三相瓦時計（A+→A− 等三相電流、電壓對 N，讀值 Wh／kWh／J；三相三線時 N 可接任一點）。
- **固定參考值／偏移**用 `CONST`（`v` 可用參數名稱），不要用功率電壓源接控制節點。控制電路用的電源（`VSIN`、`VSQU`、`VTRI` 等）負端必須接地。MCU 方波用 `VSQU` 或 `VPWL`（`times`、`vals` 兩個空白分隔清單，同一時間兩點＝跳變）。
- **參數檔** `PARAMS`（`text` 每行 `名稱 = 運算式`）：元件數值寫名稱即可；**參數掃描** `PSWEEP`（`var`、`vals`）與**蒙地卡羅** `MONTECARLO`（`vars` 每行 `R1.R 5% gauss`、`C1 10% uniform`、或參數檔變數；`n`、`seed`）都靠它或元件參數。
- **AC 掃頻／波德圖**：放 `ACSWEEP`（`src`＝激勵源名稱，`method` 0＝小訊號注入法（可分析開關電路）、1＝線性頻域法（只能線性電路））與 `ACP` 探棒（迴路增益用 `ACP2`：圓點端接注入之後、另一端接注入之前）；激勵源用 `VSIN` 且 `Vm=0`。
- **時間步長**：比最快的有意義時間常數或邊緣小 10 倍以上；開關電源取開關週期的 1/100 以下（50 kHz → dt ≤ 0.2 µs）；SPICE 類比電路 1 µs 常見。`SIMCTRL` 的自動細分步長（`lte`）、切換瞬間內插（`interp`）、切換後阻尼（`damp`）保持預設開啟；訊息提到「細分到上限仍超過容許值」代表 dt 太大，要縮小 dt。需要指定精度時用高精度模式 `acc=1`、`etol`。
- **控制演算法**：`PI`、`LIM`、`SUB2`、濾波器、`TFS`、`MATH1`～`MATH10`（運算式）、`CBLOCK`（簡化 C；接腳數由 `nin`／`nout` 決定：輸入 x1… 在左、輸出 y1… 在右，自 y = −⌊(n−1)/2⌋ 起每格一隻）。數位控制：`ZOH`、`UDELAY`、`ID`、`FILTD`／`TFZ`、`FIR`（取樣率 `fs`）。閉迴路的代數迴路用 `UTD` 斷開。除法加下限（`max(x, 1e-3)`），啟動時為 0 會產生 NaN。

### 3.1 已知注意事項

- **電容直接並聯電壓源**：初值不一致時程式以近似值起算並警告。輸入電容用 `RC`（串聯 ESR，例如 10–50 mΩ）較穩。
- `VCCVS`／`ICCCS` 的控制端（c+／c−）必須跨在一個 R／L／C 支路兩端；沒有支路可跨時用串接式 `VCCVS1`／`ICCCS1`（電流 in→out）。
- **隔離的次級側**（變壓器、光耦之後）建議用大電阻（例如 10 MΩ）或 Y 電容＋大電阻參考到初級地，避免浮接節點造成的數值問題。**Δ 接的變壓器繞組形成迴路**時，`TF3YD` 的二次側要給一點漏感或電阻（`ls`／`rs`）。
- 理想開關的關斷電阻預設 1 MΩ（PSIM 相同）；100 kΩ 等級的高阻抗類比電路把 `roff` 改大（1G 以上），否則漏電流造成誤差。
- t = 0：電感為電流源（`i0`）、電容為電壓源（`v0`）；給接近實際的初值可縮短啟動暫態。
- 三相：`VSIN3` 的 `V` 是線電壓有效值；逆變器的 DC bus 用兩顆 `RC` 分裂、中點接地；電網中性點浮接（三線制）時用 `LABEL` 而不是直接接地。

## 4. 版面與可讀性規範（必做，不必等使用者要求）

1. **分區**：主回路（輸入 → 功率級 → 輸出 → 負載）一區、控制／回授一區（可再分子區）、負載步階、分析元件各自一區。每區用 `TEXT` 放標題（如「【主回路 Power Stage】」「【控制區 Control】」），隔離處用 `TEXT` 畫界線。
2. **區內一律導線直連，不用標籤**：用長水平「軌」（BUS、GND、輸出軌）讓元件以 T 接點掛上去，不要每隻腳各拉一個 LABEL。
3. **跨區只用最少的介面標籤**，名稱就是訊號名（例如 `GATE`、`ISENS`、`VO`）；在頂部 `TEXT` 列出介面清單。**每個介面標籤兩側都要放**（`single_labels` 必須為空）。
4. **隔離地**（次級地）在主回路區內繞到邊緣，不要與初級地軌重疊或交叉，只在介面處放標籤。
5. **探棒**接在被量的節點上（接腳落在軌的內部即 T 接）或集中在底部一條帶以短導線＋同名標籤取點；名稱用網路名（Vo、Vds、IL…）。
6. **間距**：元件間 ≥ 4 格、平行支路 ≥ 5 格，名稱與參數文字周圍留 2–3 格；垂直二端元件用 rot=1；訊號左 → 右，功率上 → 下。
7. **避免**：重複或重疊的線段、導線端點剛好落在無關接腳上、導線穿過元件本體、標籤疊在標籤或元件名稱上。
8. 流程：先決定軌的座標 → 放元件 → `d.pin()` 取接腳 → `d.W()` 連到軌；短接出用 `d.stub()`／`d.tie()`。
9. 交付前**實際看截圖**（第 6.2 節）：元件是否重疊、有無不該有的交叉、分區是否清楚；看不到的區域要說明。

## 5. 工作流程

1. 釐清電路：逐一列出元件、數值、料號與每個節點的連接；看不清楚的（極性、腳位）列為假設，交付時說明。
2. 選模型（第 3 節），決定激勵、探棒（用網路名）與模擬時間；規劃分區與介面標籤（第 4 節）。
3. 用 `psimx_builder.py` 寫一個產生腳本（參考 `examples/buck_converter.py`），`d.check()` 的 `errors`、`open_pins`、`single_labels` 全部為空才存檔。
4. `verify_psimx.py` 模擬：沒有錯誤；警告要逐條看懂（不可忽略「細分到上限」「開關未收斂」）；探棒數值合理（穩態值、時間常數、步階響應），必要時用 `--expect` 寫成判定並與手算比對。**回授或感測探棒讀值接近 0、輸出遠離目標 → 多半是介面標籤只放一邊或接錯腳。**
5. 截圖目視（第 6.2 節），必要時調整版面；改版面後重跑第 4 步，數值必須與之前相同。
6. 交付 `.psimx` 與簡短說明（先講結論）：「檔案 › 開啟」或拖曳載入、F5 模擬、驗證數據、介面標籤清單、假設、未目視的部分。

## 6. 腳本

### 6.1 產生器（Python 3，只用標準函式庫）

```python
import sys; sys.path.insert(0, r'<skill>/scripts')
from psimx_builder import Doc
d = Doc(); E, W, P = d.E, d.W, d.pin
d.text(2, 1, '【主回路 Power Stage】')
E('SIMCTRL', 46, 3, 0, {'dt': '0.2u', 'tt': '10m'})
vin = E('VDC', 4, 13, 0, {'V': 24}, 'Vin')          # + (4,11)  − (4,15)
q1 = E('MOSFET', 12, 10, 0, {}, 'Q1')               # D (12,8)  S (12,12)  G (10,10)
W(P(vin, '+'), (4, 6), (12, 6), P(q1, 'D'))         # 匯流排軌
co = E('RC', 28, 17, 1, {'R': '10m', 'C': '100u'}, 'Co')   # rot 1：接腳 (28,14) 與 (28,20) 落在兩條軌內 → T 接
d.tie(q1, 'G', 'GATE', 'l', 2)                      # 介面標籤（另一側也要有 GATE）
E('VP', 40, 12, 0, {}, 'Vo')                        # 探棒接腳 (40,14)＝輸出軌端點
rep = d.check(); assert not (rep['errors'] or rep['open_pins'] or rep['single_labels']), rep
d.screens = [['Vo'], ['IL']]
d.save('circuit.psimx')
```

- `E(type, x, y, rot, params, name)`：參數名寫錯或缺少必要的名稱會直接報錯；名稱不分大小寫不可重複。
- `pin(el, name)`、`W(*points)`、`label(pt, text)`、`gnd(pt)`、`text(x, y, s)`、`stub(pt, 'l'|'r'|'u'|'d', net, length)`、`tie(el, pin, net, dir, length)`（net＝`'GND'` 時放接地）。
- `check()`：`errors`（未知型別、重複名稱、斜線、沒有 SIMCTRL、功率與控制直接相連、兩個控制輸出接同一點）、`open_pins`、`single_labels`、`warnings`（重複導線、元件疊在同一點、沒有探棒）。
- 既有檔案：`python scripts/psimx_builder.py circuit.psimx` 做同樣的檢查。

### 6.2 模擬驗證與截圖

```
python scripts/verify_psimx.py circuit.psimx [--tt 20m] [--window 0.8 1.0] [--expect Vo:avg10:11.5:12.5] [--ac]
python scripts/shot_psimx.py circuit.psimx out/shot [--run]
```

- `verify_psimx.py`：靜態檢查 → `daedalus-cli` 模擬（找 `--cli`、環境變數 `DAEDALUS_CLI`、安裝位置 `%LOCALAPPDATA%\Programs\DaedalusEngine\daedalus-cli.exe`、PATH）→ 每支探棒的 min／max／final／avg10（最後 10 %）／avg（`--window` 指定的比例區間）、警告與步數。`--tt` 暫時改總時間（快速試跑）；`--ac` 跑 AC 掃頻並列出各探棒的增益範圍。結束代碼 0＝檢查乾淨、模擬成功、`--expect` 全部成立。
- `shot_psimx.py`：用 DaedalusEngine 開檔（自動縮放到整張電路）存成 `<prefix>_main.png`；`--run` 再模擬並存波形檢視器 `<prefix>_result.png`。需要桌面環境。用看圖工具實際看截圖。
- 無法執行程式時：只交付靜態檢查結果，明確告知「未經模擬驗證」。

## 7. 元件參考

全部元件（型別代碼、中文名、接腳座標與類型、參數預設值、說明）見 `references/elements.md`；程式內「說明 › 使用說明書」（F1）另有每個元件的示範電路與波形圖、運算式與 C 程式方塊支援的函數。
