---
title: "Docker"
---

镜像只包含 `blockscope`，不包含任何 Minecraft 资产。客户端 jar、资源包和 job 目录通过挂载提供。

## 构建

```bash
docker build -t blockscope .
```

默认镜像为 CPU-only。GPU 镜像需要宿主机驱动、设备透传和可选特性：

```bash
docker build --build-arg GPU=1 -t blockscope:gpu .
```

Linux 上需要支持 Vulkan 或 D3D12 的宿主和对应设备节点（例如
`--device /dev/dri`）；镜像不包含驱动。GPU 输出为近似结果，`auto` 在看不到
适配器时仍会回退到 CPU。

MP4/GIF 任务需要容器内有 ffmpeg 4.4 或更新版本。默认镜像不包含 ffmpeg；请在
派生镜像中安装，或挂载静态二进制，并在不在 `PATH` 中时传入 `--ffmpeg`。

## 容量检测

```bash
docker run --rm --cpus=1 --memory=1g blockscope capacity --json
```

## 运行 worker

```bash
docker run --rm -i \
  --cpus=2 --memory=2g \
  -v /host/client.jar:/assets/client.jar:ro \
  -v /host/jobs:/data \
  blockscope serve --client-jar /assets/client.jar
```

容器内工作目录是 `/data`。输入路径和 `outputDir` 需要使用容器内可见的绝对路径。

## 运行 supervisor

```bash
docker run --rm -i \
  --cpus=4 --memory=4g \
  -v /host/client.jar:/assets/client.jar:ro \
  -v /host/jobs:/data \
  blockscope supervise \
    --workers 2 --threads 2 \
    --client-jar /assets/client.jar
```

## 烟雾测试

```bash
printf '%s\n' '{"type":"shutdown"}' |
  docker run --rm -i blockscope serve
```

## 资源建议

`capacity` 会根据 cgroup 的 CPU/内存限制给出 worker、thread 和默认分辨率建议。 1 CPU / 1 GiB 环境建议使用单 worker、单线程和较低分辨率；更大机器再提高并发。

完整部署说明见 [部署文档](/deployment)。

## 相关文档

- [服务模式](./service.md)
- [限制与安全](./limits.md)
- [English](/user/docker)
