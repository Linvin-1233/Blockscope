---
title: "故障排查"
---

## 常见问题

| 现象                                          | 处理                                                              |
| --------------------------------------------- | ----------------------------------------------------------------- |
| `ASSET_SOURCE_REQUIRED`                       | `materials` 需要 `--client-jar`、`--assets` 或自包含资源包        |
| `INPUT_TOO_LARGE`                             | 文件超过输入上限；确认文件完整，或调整 LOD/资源上限               |
| `MESH_LIMIT_EXCEEDED` / `PAGE_LIMIT_EXCEEDED` | 显式使用 `--lod 2`/`4`，或在可信环境中使用 `--force-limits`       |
| 画面全是纯色块                                | 没有提供资源包，或对应模型/贴图缺失；运行 `materials --json` 检查 |
| 大量 missing textures                         | 客户端 jar 版本与模型不匹配，或 overlay 资源包不完整              |
| `INVALID_RENDER_OPTIONS: threads`             | `--threads` 或请求中的线程数超过 256                              |
| `INVALID_NBT` / `INVALID_LITEMATIC`           | 文件损坏、版本不支持或结构不符合预期；先运行 `validate`           |
| `serve` 没有输出 `hello`                      | 查看 worker stderr；检查资源包路径和启动参数                      |
| 输出目录无法写入                              | 确认容器/服务用户对 `outputDir` 有写权限                          |
| 渲染很慢或内存不足                            | 使用 `capacity`、降低分辨率/超采样、减少并发或启用 LOD            |

## 排查命令

```bash
blockscope validate model.litematic
blockscope inspect model.litematic --json
blockscope stats model.litematic --json
blockscope materials model.litematic --client-jar client.jar --json
blockscope capacity --json
```

`inspect` 的 `diagnostics` 会报告 metadata 不一致、region 重叠、被忽略的实体/ tick 等问题。`validate` 适合先确认文件能否解析。

## 获取帮助

提交 issue 时请附上：

- 操作系统、Rust 版本和 `blockscope --version`
- 完整命令或协议请求（删除敏感路径）
- 实际结果和预期结果
- 错误码和关键 stderr
- 如果可能，提供最小化、可公开的合成 fixture

安全问题请通过维护者的 GitHub 资料页联系，不要先开包含敏感细节的公开 issue。

## 相关文档

- [限制与安全](./limits.md)
- [服务模式](./service.md)
- [English](/user/troubleshooting)
