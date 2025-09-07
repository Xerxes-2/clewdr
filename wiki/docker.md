# Docker Deployment Guide (Slim vs -db)

This page explains how to use the two image variants published by CI:

- Slim (config-file/env only): `ghcr.io/<owner>/<repo>:<tag>`
- DB variant (SQLite/Postgres/MySQL): `ghcr.io/<owner>/<repo>:<tag>-db`

## Slim (no DB)

Ephemeral (env-only):

```bash
docker run --rm -p 8484:8484 \
  -e CLEWDR_NO_FS=true \
  -e CLEWDR_PASSWORD=your_api_password \
  -e CLEWDR_ADMIN_PASSWORD=your_admin_password \
  ghcr.io/<owner>/<repo>:latest
```

With persistent config under `/etc/clewdr`:

```bash
docker run -p 8484:8484 \
  -v clewdr:/etc/clewdr \
  -e CLEWDR_PASSWORD=your_api_password \
  -e CLEWDR_ADMIN_PASSWORD=your_admin_password \
  ghcr.io/<owner>/<repo>:latest
```

## DB variant

SQLite (store DB alongside config):

```bash
docker run -p 8484:8484 \
  -v clewdr:/etc/clewdr \
  -e CLEWDR_PERSISTENCE__MODE=sqlite \
  -e CLEWDR_PERSISTENCE__SQLITE_PATH=/etc/clewdr/clewdr.db \
  -e CLEWDR_ADMIN_PASSWORD=your_admin_password \
  ghcr.io/<owner>/<repo>:latest-db
```

Postgres:

```bash
docker run -p 8484:8484 \
  -e CLEWDR_PERSISTENCE__MODE=postgres \
  -e CLEWDR_PERSISTENCE__DATABASE_URL=postgres://user:pass@host:5432/db \
  -e CLEWDR_ADMIN_PASSWORD=your_admin_password \
  ghcr.io/<owner>/<repo>:latest-db
```

MySQL/MariaDB:

```bash
docker run -p 8484:8484 \
  -e CLEWDR_PERSISTENCE__MODE=mysql \
  -e CLEWDR_PERSISTENCE__DATABASE_URL=mysql://user:pass@host:3306/db \
  -e CLEWDR_ADMIN_PASSWORD=your_admin_password \
  ghcr.io/<owner>/<repo>:latest-db
```

### Notes

- Nested env keys use double underscores (e.g. `CLEWDR_PERSISTENCE__MODE`).
- On first start, the DB user must have DDL permission to create tables and indexes.
- For SQLite, mount a volume on `/etc/clewdr`.

