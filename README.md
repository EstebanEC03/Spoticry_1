# 🎵 SpotiCry

> Reproductor de música por streaming con arquitectura cliente-servidor sobre WebSocket, backend funcional en Rust y frontend reactivo en React.

---

## Tabla de contenidos

- [Descripción del proyecto](#descripción-del-proyecto)
- [Arquitectura](#arquitectura)
- [Requerimientos](#requerimientos)
- [Estructura del repositorio](#estructura-del-repositorio)
- [Instalación y configuración](#instalación-y-configuración)
- [Cómo usar — Backend](#cómo-usar--backend)
- [Cómo usar — Frontend](#cómo-usar--frontend)
- [Protocolo WebSocket](#protocolo-websocket)
- [CLI del servidor](#cli-del-servidor)
- [Stack tecnológico](#stack-tecnológico)

---

## Descripción del proyecto

**SpotiCry** es una aplicación de reproducción de música que transmite audio MP3 en tiempo real desde un servidor Rust hacia un cliente web React, utilizando WebSocket como canal de comunicación bidireccional.

El backend sigue un paradigma **funcional**: las estructuras de datos del dominio (canciones, playlists) se modelan como valores inmutables, las transformaciones (sort, filter, dedupe, reverse, take, drop) retornan colecciones nuevas sin mutar la fuente, y el estado compartido se gestiona mediante snapshots atómicos protegidos por `RwLock`.

El frontend construye un reproductor completo con `MediaSource API`, buffer local de chunks de audio, cola de reproducción, y una interfaz de usuario oscura y responsiva inspirada en diseños modernos de streaming.

---

## Arquitectura

```
┌───────────────────────────────┐          WebSocket (JSON)          ┌───────────────────────────────┐
│         FRONTEND              │ ◄──────────────────────────────► │          BACKEND              │
│  React 19 + Vite 8            │    ws://localhost:7878/            │  Rust (Tokio + Tungstenite)   │
│                               │                                   │                               │
│  ┌─────────┐  ┌────────────┐  │    Requests: HELLO, LIST_SONGS,   │  ┌─────────┐  ┌────────────┐  │
│  │  Views   │  │  usePlayer │  │      PLAY_SONG, STOP_SONG, ...   │  │ Handler │  │ Controller │  │
│  │ Library  │  │ MediaSource│  │                                   │  │ (route) │─►│  (logic)   │  │
│  │Playlists │  │  + Audio   │  │    Responses: JSON envelopes      │  │         │  │            │  │
│  │  Player  │  │  Buffer    │  │    + STREAM_CHUNK (base64 audio)  │  └────┬────┘  └─────┬──────┘  │
│  └─────────┘  └────────────┘  │                                   │       │              │         │
│                               │                                   │  ┌────▼──────────────▼──────┐  │
│  services/   hooks/           │                                   │  │   Service (pure logic)   │  │
│  socket.js   usePlayer.js     │                                   │  │   + Domain (immutable)   │  │
│  api.js      storage.js       │                                   │  └────────────┬─────────────┘  │
│                               │                                   │               │                │
│                               │                                   │  ┌────────────▼─────────────┐  │
│                               │                                   │  │  Repository (JSON files) │  │
│                               │                                   │  │  data/library.json       │  │
│                               │                                   │  │  data/playlists.json     │  │
│                               │                                   │  └──────────────────────────┘  │
└───────────────────────────────┘                                   └───────────────────────────────┘
```

---

## Requerimientos

### Sistema

| Componente | Versión mínima |
|---|---|
| **Rust** | Edition 2024 (rustc ≥ 1.85) |
| **Node.js** | ≥ 18.x |
| **npm** | ≥ 9.x |
| **Sistema operativo** | Linux, macOS, Windows (WSL recomendado) |

### Backend — Dependencias (Cargo)

| Crate | Uso |
|---|---|
| `tokio` (full) | Runtime asíncrono, tareas concurrentes, señales |
| `tokio-tungstenite` | Servidor WebSocket |
| `futures-util` | Utilidades para streams y sinks asíncronos |
| `serde` + `serde_json` | Serialización/deserialización JSON |
| `uuid` (v4) | Generación de IDs únicos |
| `chrono` | Timestamps con soporte serde |
| `im` | Colecciones inmutables persistentes (`Vector`, `OrdMap`) |
| `base64` | Codificación de chunks de audio |
| `symphonia` (mp3) | Lectura de metadatos ID3 y duración de archivos MP3 |

### Frontend — Dependencias (npm)

| Paquete | Uso |
|---|---|
| `react` 19.x | Biblioteca UI |
| `react-dom` 19.x | Renderizado DOM |
| `vite` 8.x | Bundler y servidor de desarrollo |
| `@vitejs/plugin-react` | Plugin de React para Vite |
| TailwindCSS (CDN) | Sistema de estilos (cargado desde CDN en `index.html`) |
| Google Fonts (Inter) | Tipografía |
| Material Symbols | Iconografía |

---

## Estructura del repositorio

```
spoticry/
├── backend/
│   ├── Cargo.toml              # Manifiesto Rust con dependencias
│   ├── data/
│   │   ├── library.json        # Persistencia de la biblioteca de canciones
│   │   └── playlists.json      # Persistencia de playlists
│   ├── songs/                  # Archivos MP3 fuente
│   │   ├── Keys Of Moon - The Epic Hero.mp3
│   │   ├── Makai Symphony - Dragon Castle.mp3
│   │   └── Scott Buckley - Filaments.mp3
│   └── src/
│       ├── main.rs             # Punto de entrada, bootstrap del servidor
│       ├── error.rs            # Tipo de error de aplicación
│       ├── cli/                # Interfaz de línea de comandos del servidor
│       ├── controllers/        # Traducción de requests a responses
│       ├── domain/             # Modelos de dominio inmutables
│       ├── network/            # Capa de red WebSocket
│       ├── repositories/       # Persistencia a disco (JSON)
│       ├── services/           # Lógica de negocio pura
│       └── states/             # Estado global compartido
├── frontend/
│   ├── index.html              # Shell HTML con config de Tailwind
│   ├── package.json            # Dependencias Node.js
│   ├── vite.config.js          # Configuración de Vite
│   └── src/
│       ├── main.jsx            # Punto de entrada React
│       ├── App.jsx             # Componente raíz, estado global, routing
│       ├── App.css             # Estilos base
│       ├── index.css           # Reset CSS
│       ├── components/         # Componentes de UI
│       │   ├── Sidebar.jsx     # Navegación lateral
│       │   ├── LibraryView.jsx # Vista de biblioteca con filtros
│       │   ├── PlaylistsView.jsx # Vista de playlists con operaciones
│       │   └── Player.jsx      # Reproductor de audio
│       ├── hooks/
│       │   └── usePlayer.js    # Hook del reproductor (MediaSource API)
│       └── services/
│           ├── socket.js       # Cliente WebSocket con reconexión
│           ├── api.js          # Funciones de protocolo
│           └── storage.js      # Persistencia local (localStorage)
├── .gitignore
└── .gitattributes
```

---

## Instalación y configuración

### 1. Clonar el repositorio

```bash
git clone https://github.com/EstebanEC03/spoticry.git
cd spoticry
```

### 2. Backend

```bash
cd backend
cargo build --release
```

### 3. Frontend

```bash
cd frontend
npm install
```

---

## Cómo usar — Backend

### Iniciar el servidor

```bash
cd backend
cargo run
```

El servidor:
1. Carga `data/library.json` y `data/playlists.json` desde disco.
2. Inicia un servidor WebSocket en `ws://0.0.0.0:7878/`.
3. Lanza un CLI interactivo en la terminal para administración directa.
4. Al recibir `Ctrl+C` o el comando `quit`, persiste un snapshot final de library y playlists a disco.

### Agregar canciones

Las canciones se agregan **exclusivamente** a través del CLI del servidor. El archivo MP3 debe existir en el sistema de archivos local.

```bash
# Agregar con detección automática de metadatos ID3
add ./songs/mi-cancion.mp3

# Agregar con overrides manuales (pipe-separated)
add ./songs/mi-cancion.mp3 | Mi Título | Mi Artista | Rock
```

El servidor valida que:
- El archivo exista y sea un MP3 válido (contenido, no solo extensión).
- No sea un duplicado (misma ruta canónica).
- Pueda extraer la duración del stream.

### Directorio de canciones

Colocar los archivos MP3 en `backend/songs/` (o cualquier ruta accesible). El servidor almacena rutas absolutas canónicas en `library.json`.

---

## Cómo usar — Frontend

### Iniciar el servidor de desarrollo

```bash
cd frontend
npm run dev
```

Vite inicia en `http://localhost:5173` por defecto. El frontend se conecta automáticamente al backend en `ws://<hostname>:7878/`.

### Navegación

| Vista | Descripción |
|---|---|
| **Library** | Lista todas las canciones con filtros por título, artista, género y rango de duración. Click en una fila para reproducir. |
| **Playlists** | Crear, ver, eliminar playlists. Dentro de una playlist: sort, filter, reverse, dedupe, take, drop. |
| **Now Playing** | Vista ampliada de la canción actual con progreso y estado. |

### Reproducción de audio

1. Haz click en el botón de play (▶) en cualquier fila de canción.
2. El frontend envía `PLAY_SONG` al servidor.
3. El servidor responde con metadata + comienza a enviar `STREAM_CHUNK` frames (base64-encoded, 16 KiB).
4. El hook `usePlayer` decodifica los chunks y los alimenta a un `MediaSource` / `SourceBuffer`.
5. Controles disponibles: play/pause, stop, seek (dentro del rango buffereado), ±10s, anterior/siguiente en cola, volumen.

### Filtros de la biblioteca

- **Título / Artista** (client-side): búsqueda por substring, case-insensitive.
- **Género** (client-side): match exacto mediante dropdown.
- **Duración** (server-side): rango numérico enviado como criterio `range` al servidor vía `LIST_SONGS`.

### Operaciones de playlist

Desde la vista de detalle de una playlist:

| Operación | Tipo | Descripción |
|---|---|---|
| **Sort** | Derivada | Ordena por título, artista, duración o fecha. Asc/Desc. |
| **Filter** | Derivada | Filtra por género exacto. |
| **Reverse** | Derivada | Invierte el orden de las canciones. |
| **Dedupe** | Derivada | Elimina duplicados manteniendo primera aparición. |
| **Take N** | Derivada | Toma las primeras N canciones. |
| **Drop N** | Derivada | Descarta las primeras N canciones. |

> **Nota**: Las operaciones derivadas no mutan la playlist original en el servidor. Muestran una vista temporal.

---

## Protocolo WebSocket

Comunicación bidireccional JSON sobre WebSocket. Cada request incluye un `id` y un `type`; cada response correlaciona el mismo `id`.

### Operaciones soportadas

| Tipo | Dirección | Descripción |
|---|---|---|
| `HELLO` | C → S | Handshake con `client_id` y `version` |
| `PING` | C → S | Latencia; responde `PONG` |
| `LIST_SONGS` | C → S | Listar canciones, opcionalmente con criterio |
| `GET_SONG` | C → S | Obtener detalle de una canción |
| `PLAY_SONG` | C → S | Iniciar streaming de una canción |
| `STOP_SONG` | C → S | Detener un stream activo |
| `CREATE_PLAYLIST` | C → S | Crear nueva playlist |
| `DELETE_PLAYLIST` | C → S | Eliminar playlist |
| `LIST_PLAYLISTS` | C → S | Listar playlists (opcionalmente por owner) |
| `GET_PLAYLIST` | C → S | Obtener detalle de playlist con canciones |
| `ADD_SONG_TO_PLAYLIST` | C → S | Agregar canción a playlist |
| `REMOVE_SONG_FROM_PLAYLIST` | C → S | Remover canción de playlist |
| `FILTER_PLAYLIST` | C → S | Filtrar canciones de una playlist |
| `SORT_PLAYLIST` | C → S | Ordenar canciones de una playlist |
| `TRANSFORM_PLAYLIST` | C → S | Transformar playlist (dedupe, reverse, take, drop) |
| `STREAM_CHUNK` | S → C | Chunk de audio base64 (no solicitado) |
| `LIBRARY_UPDATED` | S → C | Notificación broadcast al agregar/remover canciones |

### Formato de respuesta

```json
{
  "id": "request-uuid",
  "type": "LIST_SONGS",
  "status": "ok",
  "data": { "count": 3, "songs": [...] }
}
```

```json
{
  "id": "request-uuid",
  "type": "PLAY_SONG",
  "status": "error",
  "error": { "code": "NOT_FOUND", "message": "Song abc not found" }
}
```

---

## CLI del servidor

Comandos disponibles cuando el backend está en ejecución:

| Comando | Uso |
|---|---|
| `add <path> [│ title │ artist │ genre]` | Agregar MP3 a la biblioteca |
| `remove <song_id>` | Remover canción (falla si está en streaming) |
| `list` | Listar todas las canciones |
| `playlists` | Listar todas las playlists |
| `create-playlist <name> [│ owner]` | Crear playlist (owner por defecto: "cli") |
| `delete-playlist <playlist_id>` | Eliminar playlist por ID |
| `help` | Mostrar ayuda |
| `quit` | Guardar snapshots y salir |

---

## Stack tecnológico

| Capa | Tecnología |
|---|---|
| **Backend runtime** | Rust + Tokio (async) |
| **Protocolo** | WebSocket (tokio-tungstenite) |
| **Serialización** | serde + serde_json |
| **Audio parsing** | Symphonia (ID3 tags + duración) |
| **Estructuras inmutables** | im (Vector, OrdMap) |
| **Frontend framework** | React 19 |
| **Bundler** | Vite 8 |
| **Estilos** | TailwindCSS (CDN) |
| **Audio playback** | MediaSource API + SourceBuffer |
| **Persistencia servidor** | JSON-on-disk (escritura atómica vía temp+rename) |
| **Persistencia cliente** | localStorage |
