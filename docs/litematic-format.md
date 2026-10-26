# Litematic Format Notes

本文记录 Phase 1 已验证或明确采用的 `.litematic` 语义。实现参考 Litematica
上游（LGPL-3.0）的公开源码，不复制其代码：

- [LitematicaSchematic.java](https://github.com/maruohon/litematica/blob/dc426db5b2ff93cc56790287d203d03944facf5d/src/main/java/fi/dy/masa/litematica/schematic/LitematicaSchematic.java)
- [LitematicaBitArray.java](https://github.com/maruohon/litematica/blob/dc426db5b2ff93cc56790287d203d03944facf5d/src/main/java/fi/dy/masa/litematica/schematic/container/LitematicaBitArray.java)
- [SchematicMetadata.java](https://github.com/maruohon/litematica/blob/dc426db5b2ff93cc56790287d203d03944facf5d/src/main/java/fi/dy/masa/litematica/schematic/SchematicMetadata.java)
- 旧版 tight packing 对照：[TightLongBackedIntArray.java](https://github.com/maruohon/litematica/blob/27efd02b5f691a8999f1ac3854269f9fa202b5ca/src/main/java/litematica/schematic/container/TightLongBackedIntArray.java)

`Standalone-Litematic-Renderer` 没有 LICENSE，只用于观察行为，不作为代码或
资产来源。

## 1. 顶层结构

根 tag 是 Compound：

```text
Version                 Int
SubVersion              Int, optional, v7 引入
MinecraftDataVersion    Int
Metadata                Compound
Regions                 Compound<regionName, Region>
```

`Version` 当前实现支持 `1..=7`。缺失或超出范围返回
`UNSUPPORTED_LITEMATIC_VERSION`。`MinecraftDataVersion` 缺失时记为 `-1`。

`Metadata` 和 `Regions` 缺失、类型错误或 `Regions` 为空时返回
`INVALID_LITEMATIC`。

## 2. Metadata

我们读取以下字段，缺失时保留 `None`。这些值只作为提示，统计以实际方块数据
为准：

```text
Name, Author, Description
EnclosingSize       Compound{x,y,z} 或 IntList[3]
Origin              Compound{x,y,z}
TimeCreated, TimeModified
RegionCount
TotalVolume, TotalBlocks
BlockEntityCount, BlockTickCount
McDataVersion, McVersion
PreviewImageData    IntArray, 只记录长度
```

`RegionCount`、`TotalVolume`、`TotalBlocks`、`BlockEntityCount`、
`BlockTickCount` 与实际计算不一致时，产出 `METADATA_MISMATCH` 诊断。
`EnclosingSize` 与场景 bounds 不一致时同样产出诊断。

## 3. Region 字段

每个 region 是 Compound：

```text
Position            Compound{x,y,z}
Size                Compound{x,y,z}，允许负数
DataVersion         Int, optional
BlockStatePalette   List<Compound>
BlockStates         LongArray
TileEntities        List<Compound>, optional
Entities            List<Compound>, optional
PendingBlockTicks   List<Compound>, v3+
PendingFluidTicks   List<Compound>, v5+
```

`Position` 和 `Size` 也接受 `List<Int>[3]` 作为兼容形式。

### 3.1 负尺寸与坐标归一化

Litematica 的 `Size` 是从 `Position` 指向对角的方向向量，因此可以是负数。
归一化规则：

```text
end_offset = size > 0 ? size - 1 : size + 1
end        = position + end_offset
min        = min(position, end)
max        = max(position, end)
abs_size   = max - min + 1
```

示例：`Position = [10,20,30]`，`Size = [-2,-3,-4]`，则
`origin = [9,18,27]`，`size = [2,3,4]`，体积 `24`。

任一轴 `Size == 0`、坐标超出配置范围或体积溢出时返回
`INVALID_REGION_BOUNDS`。

### 3.2 索引顺序

局部方块索引使用 x 最快、z 次之、y 最慢：

```text
index = y * sizeX * sizeZ + z * sizeX + x
```

## 4. BlockStatePalette 与 BlockStates

### 4.1 Palette

每个 palette 项是 Compound：

```text
Name        String, 例如 minecraft:oak_stairs
Properties  Compound<String, String>, optional
```

规则：

- 没有命名空间时补 `minecraft:`。
- `Properties` 按键排序存入 `BTreeMap`，形成规范 `BlockStateKey`。
- 空名称、含空白、多个冒号或非字符串属性返回
  `UNSUPPORTED_PALETTE_FORMAT` / `INVALID_PALETTE`。
- 不执行 Minecraft datafixer 重命名。pre-1.13.2 数据版本保留原始名称并产出
  `UNSUPPORTED_DATA_VERSION` 诊断。

### 4.2 位宽

```text
bits = max(2, ceil(log2(palette_len)))
```

| palette 数量 | bits |
|---|---|
| 1-4 | 2 |
| 5-8 | 3 |
| 9-16 | 4 |
| 17-32 | 5 |
| 33-64 | 6 |
| 65-128 | 7 |

### 4.3 Packing

`BlockStates` 使用紧密 LSB-first packing，条目可以跨 64-bit 边界：

```text
start_offset = index * bits
start_array  = start_offset / 64
start_bit    = start_offset % 64
end_array    = ((index + 1) * bits - 1) / 64
```

同一 long 内直接右移取值；跨 long 时把下一个 long 左移
`64 - start_bit` 后拼接。`bits` 最大为 32，mask 为 `(1 << bits) - 1`。

`LongArray` 长度必须等于：

```text
ceil(volume * bits / 64)
```

否则返回 `BLOCK_STATE_DATA_LENGTH_MISMATCH`。每个索引必须小于 palette 长度，
否则返回 `INVALID_PALETTE_INDEX`，错误信息包含局部坐标。

示例：`bits = 2`，值 `[0,1,2]`，打包结果为 `0b10_01_00 = 36`。

## 5. Block Entity

### 5.1 v1

`TileEntities` 元素是 wrapper：

```text
TileNBT     Compound，真正的方块实体 NBT
x, y, z     Int，region 局部坐标
```

### 5.2 v2+

`TileEntities` 元素直接是方块实体 NBT：

```text
id          String
x, y, z     Int，region 局部坐标
...         其余方块实体字段
```

位置也接受 `Pos: List<Int>[3]` 作为兼容形式。

### 5.3 策略

- 坐标必须在 `[0, size)` 内。越界条目产出
  `OUT_OF_BOUNDS_BLOCK_ENTITY` 警告并丢弃，不让整个任务失败。
- 同一坐标重复时后一个覆盖前一个，产出
  `DUPLICATE_BLOCK_ENTITY` 警告。
- 缺少 `id` 时使用 `unknown`，产出 `MISSING_BLOCK_ENTITY_ID` 警告。
- 缺少位置或 `TileNBT` 等结构错误返回 `INVALID_BLOCK_ENTITY`。
- `BlockEntity.data` 保留完整原始 NBT，`id` 和 `local_pos` 是提取出的索引。

## 6. Entities 与 Pending Ticks

Phase 1 只计数，不执行、不渲染：

- `entity_count`
- `pending_block_tick_count`
- `pending_fluid_tick_count`

非零时产出 `ENTITIES_IGNORED` / `PENDING_TICKS_IGNORED` 信息诊断。清单中保留
计数，后续渲染阶段再决定是否支持实体或 tick 状态。

## 7. Region 顺序与重叠

`Regions` 在 NBT 中是 Compound，解码后按键名字典序排序。该顺序是确定性的：

- 渲染时按排序后的 region 顺序覆盖，**后面的 region 覆盖前面的 region**。
- 每对重叠 region 产出 `REGION_OVERLAP` 警告，包含重叠体积；最多输出 64 条，
  其余合并为一条汇总诊断。
- 材料统计按 region 分别计数，重叠部分会在每个 region 中各计一次。清单中的
  `statistics.volume` 是各 region 体积之和，`bounds.volume` 是联合包围盒体积，
  两者可能不同。

## 8. 输入限制

默认值定义在 `LitematicLimits` 和 `DecodeLimits`：

| 限制 | 默认值 |
|---|---|
| 原始输入 | 64 MiB |
| 解压后 NBT | 256 MiB |
| NBT 深度 | 64 |
| NBT tag 数 | 4,000,000 |
| region 数 | 1024 |
| 单 region 体积 | 100,000,000 |
| 总体积 | 200,000,000 |
| palette 项数 | 1,000,000 |
| 坐标绝对值 | 30,000,000 |
| Block Entity 数 | 1,000,000 |
| Entity 数 | 1,000,000 |
| Pending Tick 数 | 1,000,000 |

所有乘法、偏移和长度检查使用 checked arithmetic。超限返回 `RESOURCE_LIMIT`
或对应的专用错误。

## 9. 错误码

| 错误码 | 含义 |
|---|---|
| `INVALID_NBT` | 底层 NBT/压缩解析失败 |
| `INVALID_LITEMATIC` | 顶层结构缺失或类型错误 |
| `UNSUPPORTED_LITEMATIC_VERSION` | `Version` 不在 1-7 |
| `INVALID_METADATA` | Metadata 字段类型错误 |
| `INVALID_REGION_BOUNDS` | 零尺寸、坐标越界或边界溢出 |
| `RESOURCE_LIMIT` | 超过配置预算 |
| `INVALID_PALETTE` | palette 结构或属性类型错误 |
| `UNSUPPORTED_PALETTE_FORMAT` | 非现代 `Name`/`Properties` palette |
| `BLOCK_STATE_DATA_LENGTH_MISMATCH` | `BlockStates` 长度与体积/位宽不符 |
| `INVALID_PALETTE_INDEX` | 方块索引超出 palette 范围 |
| `INVALID_BLOCK_ENTITY` | 方块实体结构错误 |
| `BLOCK_ENTITY_LIMIT_EXCEEDED` | 方块实体数量超限 |

## 10. 验证

- `crates/litematic/tests/container.rs`：容器、metadata、负尺寸、限额。
- `crates/litematic/tests/region_data.rs`：palette、packing、block entity。
- `crates/litematic/tests/scene.rs`：Scene、统计、metadata 诊断、重叠。
- `crates/cli/tests/golden.rs`：v1/v6/v7、多 region、负尺寸、全空气、33 项
  palette 的 golden manifest。
- `crates/litematic/tests/real_files.rs`：通过 `LITEMATIC_FIXTURES_DIR` 运行
  私有真实文件，默认 ignored。
- `crates/nbt/tests/properties.rs` 与 `crates/scene/tests/properties.rs`：
  随机字节不 panic、packed index 往返。

## 11. 已知未覆盖

- pre-1.13 的 datafixer 重命名和旧 palette 变体。
- 实体与 pending tick 的执行/渲染。
- 方块实体类型化渲染（箱子已支持；告示牌、旗帜、头颅等仍需后续阶段）。
- 材质动画播放（当前只采样首帧）。
- 超过 64 对 region 重叠时诊断会汇总；完整重叠图需要后续阶段。

这些边界不影响 Phase 1 的“解析、统计、诊断、清单”目标。
