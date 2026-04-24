# Medium Packaging And Bootstrap Design

**Date:** 2026-04-24

## Goal

Сделать production-friendly установку `Medium`, в которой пользователь ставит системный пакет привычным способом и подключается к сети в 1-2 понятных шага.

Первая волна должна покрывать:
- Linux server
- Linux client
- macOS client

## Product Surface

`Medium` распространяется как пакет, а не как одноразовый install script. После установки пользователь работает с единым CLI:

- `medium init-control`
- `medium join <invite>`
- `medium devices`
- `medium ssh sync`
- `medium doctor`

В первой production версии:
- серверная роль поддерживается на Linux
- клиентская роль поддерживается на Linux и macOS
- GUI installer не делается
- mobile packaging не делается
- relay productionization не входит в этот срез

## Packaging Model

### Linux Server

Linux server package должен включать:

- бинарь `medium`
- systemd unit для `medium-control-plane`
- systemd unit для `medium-home-node`
- example config files для control-plane и node

Пакет не должен автоматически инициализировать сеть или silently запускать overlay. Явный запуск делает пользователь через `medium init-control`.

### Linux Client

Linux client package должен включать:

- бинарь `medium`
- user-facing CLI commands для `join`, `devices`, `ssh sync`, `doctor`

Для первого production slice Linux client не обязан автоматически поднимать постоянный background daemon, если пользователь выступает только потребителем сети, а не публикующим узлом.

### macOS Client

macOS client package распространяется через Homebrew. Первая волна включает:

- бинарь `medium`
- CLI команды `join`, `devices`, `ssh sync`, `doctor`

На первом этапе не делаются:

- отдельный GUI app installer
- launchd-managed background service по умолчанию

## Install And Bootstrap Flow

### Server Flow

Пользователь:

1. устанавливает пакет `medium`
2. выполняет `sudo medium init-control`
3. получает invite string или готовую join-команду для клиента

`medium init-control` должен:

- создавать директории конфигурации и состояния, если они отсутствуют
- генерировать control-plane secret material
- инициализировать SQLite database
- рендерить control-plane config
- рендерить home-node config
- включать и запускать systemd units
- создавать initial owner/admin bootstrap state
- печатать invite для клиентов

### Client Flow

Пользователь:

1. устанавливает пакет `medium`
2. выполняет `medium join '<invite>'`
3. выполняет `medium ssh sync`
4. использует `ssh <node_name>`

`medium join` должен:

- сохранять local node identity
- сохранять trust material и адрес control-plane
- регистрировать node в сети
- сохранять local config/state
- не пересоздавать identity молча, если клиент уже joined

## Paths

### Linux Server

- config: `/etc/medium`
- state: `/var/lib/medium`
- logs: `journald`

### Linux Client

- config: `~/.config/medium`
- state: `~/.local/share/medium`

### macOS Client

- config: `~/Library/Application Support/Medium/config`
- state: `~/Library/Application Support/Medium`

### SSH

- main ssh config: `~/.ssh/config`
- managed include file: `~/.ssh/config.d/medium.conf`

`medium ssh sync` управляет только `medium.conf`. Основной `~/.ssh/config` меняется только для добавления единичного `Include`, с backup при первом изменении.

## Command Semantics

### `medium init-control`

Требования:

- команда идемпотентна
- при повторном запуске не затирает существующую установку молча
- если инсталляция уже существует, команда либо подтверждает ее состояние, либо требует явный `--reconfigure`

### `medium join <invite>`

Требования:

- команда валидирует invite format и schema version
- если local node identity уже существует, она не затирается молча
- для повторного подключения используются отдельные explicit modes, например `--rejoin` или `--rotate-node`

### `medium ssh sync`

Требования:

- команда атомарно перегенерирует managed file
- перед перезаписью managed file делает backup
- основной `~/.ssh/config` меняет только при отсутствии include

### `medium doctor`

Команда должна диагностировать:

- наличие бинаря и ожидаемых путей
- наличие config/state directories
- состояние system services на Linux server
- наличие SQLite DB
- доступность control-plane
- join state клиента
- наличие SSH include и managed config

## Invite Format

Invite должен быть versioned и пригодным для эволюции протокола.

Допустимый формат для первой версии:

- `medium://join?v=1&control=https://host:port&token=...`

Альтернативное внутреннее кодирование допустимо, но логически invite обязан содержать:

- schema version
- control URL
- bootstrap token
- optional expiry
- optional server fingerprint

## Service Model

Для первой production волны:

- `control-plane` работает как system service на Linux server
- `home-node` работает как отдельный system service на Linux server
- клиентский CLI на macOS и Linux работает без обязательного background daemon

Если позже появится публикация client-side сервисов, это добавляется как отдельный срез и не меняет базовый install flow.

## Error Handling And Rollback

### Config Safety

- все пользовательские конфиги, которые Medium меняет, должны иметь backup перед первой перезаписью
- managed files могут перегенерироваться автоматически
- unmanaged user files не должны silently перетираться

### Upgrade Model

- обновление binary делает package manager
- database migrations запускаются при старте `control-plane`
- local node schema upgrades выполняются CLI или daemon при запуске

### Rollback Model

- rollback package делает package manager
- rollback managed configs возможен через backup files
- rollback DB schema не гарантируется в `v1`; поддерживаются forward migrations и backup-before-major-change

## Non-Goals

В этот срез не входят:

- mobile installers
- desktop GUI installer
- automated relay deployment
- multi-node mesh orchestration
- full zero-touch provisioning

## Acceptance Criteria

Срез считается завершенным, когда выполняется следующий flow:

1. На чистом Linux server пользователь ставит пакет `medium`
2. Выполняет `sudo medium init-control`
3. Получает invite
4. На macOS или Linux client пользователь ставит пакет `medium`
5. Выполняет `medium join '<invite>'`
6. Выполняет `medium ssh sync`
7. Выполняет `ssh <node_name>` через managed SSH config

Дополнительно:

- `medium doctor` корректно отражает состояние сервера и клиента
- повторный `init-control` не ломает установку
- повторный `ssh sync` безопасно обновляет managed config
