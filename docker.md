# Packaged as docker container

The docker setup relies heavily on @jessfraz's wireguard tools image (r.j3ss.co/wg:tools, which is based on alpine). Hence we need some cross-compilation of the exporter to run with musl.

## Build with docker

The docker build leverages multi stage build to create a small image without all build tools based on @jessfraz's wg tools image. Building in docker is as simple as:

 `docker build -t ajaegle/prometheus-wireguard-exporter:1.0.0 .`.

## Running

Exposing metrics in default mode (port mapping not relevant due to host mode)

```
docker run -it --rm \
  --log-driver none \
  -v /tmp:/tmp \
  --cap-add NET_ADMIN \
  --net host \
  --name promex-wireguard \
  ajaegle/prometheus-wireguard-exporter:1.0.0
```

Verbose and with friendly names file

```
docker run -it --rm \
  --log-driver none \
  -v /tmp:/tmp \
  -v /etc/wireguard/wg0.conf:/wg0.conf:ro \
  --cap-add NET_ADMIN \
  --net host \
  --name promex-wireguard \
  ajaegle/prometheus-wireguard-exporter:1.0.0 \
    -v \
    -n /wg0.conf
```

## Debugging

Testing wireguard tool (coming from r.j3ss.co/wg:tools)

```
docker run -it --rm \
  --log-driver none \
  -v /tmp:/tmp \
  --cap-add NET_ADMIN \
  --net host \
  --name promex-wireguard \
  --entrypoint /usr/bin/wg \
  ajaegle/prometheus-wireguard-exporter:1.0.0 \
    show all
```
