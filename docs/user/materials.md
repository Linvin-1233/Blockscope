# 材质与资源包

## 内置色块

不提供资源包时，引擎使用内置色块 catalog。它适合快速预览、CI 和回归测试，
但不会还原真实贴图。

```bash
blockscope render model.litematic --output out.png
```

## 使用客户端 jar

```bash
blockscope render model.litematic --output out.png --client-jar client.jar
```

客户端 jar 中可以包含 `assets/minecraft/blockstates`、`models` 和 `textures`。
引擎直接读取 jar，不会解压到工作目录。

## 使用已解压 assets

```bash
blockscope render model.litematic --output out.png --assets ./assets
```

`--assets` 指向包含 `assets/minecraft/...` 的目录，或已解压的 `assets/` 目录。

## 叠加资源包

```bash
blockscope render model.litematic --output out.png \
  --client-jar client.jar \
  --resource-pack ./packs/my-pack
```

`--resource-pack` 可以是目录，也可以是 zip/jar。overlay 中的同名 blockstate、
model 或 texture 会覆盖 base。

## 回退行为

资源包缺失模型或贴图时，引擎会按以下顺序回退：

1. 资源包模型
2. 内置 partial shape 或色块
3. 资源包 `particle` 纹理（如果可用）
4. 品红色 unknown 方块

使用 `materials --json` 查看覆盖率和缺失统计：

```bash
blockscope materials model.litematic --client-jar client.jar --json
```

非 JSON 的 `render` 输出也会打印材质诊断。完整材质规则见
[资源包材质说明](../textures.md)。

## 资产政策

资源包、客户端 jar 由使用者自行提供；请确保其使用符合 Minecraft EULA。本仓库
不包含 `.jar`、`resourcepacks/`、Minecraft 贴图或其他 Minecraft 资产。

## 相关文档

- [CLI 命令](./cli.md)
- [限制与安全](./limits.md)
- [故障排查](./troubleshooting.md)
- [English](./en/materials.md)
