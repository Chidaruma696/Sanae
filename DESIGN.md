# Sanae · diseño

**早苗 · Una tienda de software para Arch Linux que vive en la terminal, instala y además configura.**

Documento de diseño previo a escribir código. Objetivo: que Sanae quede bien pensada antes de integrarla con Reimu y de simplificar Reimu. Fecha: 2026-09-10.

---

## 1. Qué es Sanae y qué no es

Sanae es un gestor de paquetes con interfaz de terminal (TUI) para Arch Linux, con aspecto y flujo de "tienda" (como pamac u Octopi) pero sin X ni Qt: corre en cualquier TTY, por SSH y dentro del chroot de Reimu. Hace tres cosas:

1. **Explorar e instalar** paquetes de los repositorios oficiales, del AUR y de repos extra (Chaotic-AUR, propios), con búsqueda unificada, detalles completos, cola de cambios y ejecución con salida en vivo.
2. **Mantener** el sistema: actualizaciones (repos + AUR) con las noticias de Arch delante, huérfanos, caché, paquetes explícitos vs dependencias, historial.
3. **Configurar** lo que un paquete solo no deja listo, mediante **recetas**: fuentes con fontconfig, QEMU/KVM con libvirt y el usuario en el grupo, Docker con el servicio activo, impresión, entrada en japonés, temas de XFCE, etc. Esto es lo que ningún gestor de paquetes hace y lo que hoy está repartido dentro de Reimu.

Principios (los que pediste):

- **KISS.** Un binario, sin demonio, sin base de datos propia. Lee lo que pacman ya tiene en disco y pregunta al AUR por HTTP. La configuración y las recetas son texto plano.
- **YAGNI.** Sin Flatpak, Snap, escáneres de seguridad, votación en el AUR ni bandeja del sistema en la v1. Se añaden cuando alguien los necesite de verdad.
- **Unix.** Sanae no reimplementa pacman: lo ejecuta. Tampoco reimplementa el ayudante de AUR: usa paru o yay. Cada subcomando sirve desde scripts y devuelve JSON si se le pide. Reimu la llama como a cualquier otro programa.

Lo que Sanae **no** es: no es otro ayudante de AUR (no compila PKGBUILDs por su cuenta), no es un GUI, no sustituye a pacman en la terminal para quien ya sabe usarlo.

---

## 2. Lo que ya existe y qué aprender de cada uno

Investigado el 2026-09-10.

| Herramienta | Qué es | Lo bueno | Lo que le falta para lo que queremos |
| --- | --- | --- | --- |
| [pacseek](https://github.com/moson-mo/pacseek) (Go, tview) | Buscador TUI de repos + AUR, instala delegando en yay | Simple, rápido, caché, ver PKGBUILD, noticias antes de actualizar | Es solo búsqueda + instalar. Sin tienda, sin cola real, sin recetas, sin gestión del sistema |
| [Pacsea](https://github.com/Firstp1ck/Pacsea) (Rust, ratatui, MIT) | El más completo: búsqueda unificada, cola con "preflight", comentarios del AUR, escaneos de seguridad, ejecución dentro de la TUI con sudo/doas | Demuestra que la arquitectura Rust + ratatui + `pacman` por CLI + AUR RPC funciona y es rápida. Su autor extrajo la lógica en [arch-toolkit](https://github.com/Firstp1ck/arch-toolkit) (librería MIT) | Es una herramienta para quien ya sabe qué paquete quiere. Sin categorías, sin "populares", sin nombres humanos, sin recetas ni configuración posterior. Muchísimas funciones (escáneres, votos por SSH, noticias por distro): lo contrario de YAGNI |
| yup, SPM, pcurses, cylon | Wrappers TUI/ncurses de pacman | Ligeros | Abandonados o mínimos |
| [pamac](https://github.com/manjaro/pamac) (GTK) | La tienda de Manjaro | Categorías y nombres humanos gracias a **AppStream**, vistas Instalados/Actualizaciones, historial, cola, Flatpak | Es GTK, no hay TUI, y arrastra Flatpak/Snap |
| [Octopi](https://github.com/aarnt/octopi) (Qt) | Frontal de pacman clásico | Vistas por repo/grupo/categoría, pestañas de detalle (info, archivos, transacción, salida), marcar en lote, limpiador de caché, editor de repos, notificador | Es Qt; su "categoría" es el grupo de pacman, que casi ningún paquete tiene |
| aptitude (Debian, ncurses) | El abuelo de las TUI de paquetes | El modelo mental: árbol de categorías, marcar `+`/`-`, ver el plan de cambios antes de aplicar, resolver conflictos en un panel | Su estética es de 1999 |

Conclusión: la arquitectura técnica correcta ya está probada por Pacsea (Rust + ratatui, pacman por CLI, AUR RPC). Lo que nadie tiene en terminal es la **capa de tienda** (categorías, nombres, descripciones traducidas, popularidad) ni la **capa de recetas**. Ahí está Sanae.

---

## 3. Fuentes de datos (todas ya existen; Sanae solo las lee)

| Fuente | Cómo se accede | Para qué |
| --- | --- | --- |
| Base de datos de sincronización de pacman (`/var/lib/pacman/sync/*.db`) | `expac -S '%n\t%v\t%r\t%d\t%g\t%m'` para volcar todos los paquetes en una pasada (~15 000 filas en < 1 s); `pacman -Si pkg` para el detalle | Catálogo oficial y de repos extra, versión, repo, descripción, grupos, tamaño |
| Base de datos local (`/var/lib/pacman/local`) | `expac -Q` y `pacman -Qi/-Ql/-Qdt/-Qm` | Instalados, explícitos vs dependencias, huérfanos (`-Qdt`), paquetes ajenos = AUR (`-Qm`), archivos de un paquete |
| Archivos de paquetes no instalados | `pacman -F` / `pkgfile` | "¿Qué paquete trae el comando X?" |
| AUR RPC v5 (`https://aur.archlinux.org/rpc/v5/…`) | Probado hoy en vivo: `search/{texto}?by=name\|name-desc\|keywords\|groups\|maintainer\|depends…` e `info?arg[]=a&arg[]=b` (lote). Campos: Name, Version, Description, NumVotes, Popularity, OutOfDate, Maintainer, Submitter, License, URL, URLPath, Depends, MakeDepends, OptDepends, Keywords, FirstSubmitted, LastModified | Búsqueda y detalle del AUR, con caché en disco por 10 min |
| Archivo de metadatos del AUR (`packages-meta-ext-v1.json.gz`, 14 MB, se regenera cada pocos minutos) | Descarga opcional una vez al día | Búsqueda completa sin límite de resultados y sin golpear el RPC; modo "sin conexión" |
| PKGBUILD y comentarios del AUR | `https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h=pkg` y la página del paquete | Ver antes de instalar lo que se va a compilar |
| **AppStream** (`archlinux-appstream-data`, extra, `/usr/share/swcatalog/xml/*.xml.gz` + iconos) | Leer los XML comprimidos directamente (serde + flate2) o `appstreamcli search/dump` | **La tienda:** nombre humano ("Firefox", no "firefox"), resumen y descripción **traducidos al español**, categorías freedesktop (Audio, Development, Games, Graphics, Network, Office, Science, Utility…), palabras clave, licencia, URL, capturas (URLs). Solo cubre aplicaciones con metadatos AppStream (~2 000), que son justo las que un usuario de tienda busca |
| pkgstats (`https://pkgstats.archlinux.de/api/packages/{pkg}`) | JSON con `popularity` = porcentaje de sistemas que lo tienen instalado (probado hoy: firefox 69 %) | Ordenar por "los más usados" y una sección de destacados sin curar nada a mano |
| Noticias de Arch (`https://archlinux.org/feeds/news/`) | RSS | Mostrar las novedades que requieren intervención manual antes de actualizar, como hacen pacseek y Pacsea |
| Repos extra (Chaotic-AUR, propios) | Son bases de datos de pacman normales: aparecen solos en `expac -S` con su nombre de repo | Nada especial que hacer; Sanae los muestra con su etiqueta |

**Decisión sobre libalpm.** Existe `alpm.rs` (bindings oficiales de Rust). Hoy pacman 7.1 provee `libalpm.so=16` y la última versión publicada del crate (5.0.2, enero 2026) declara soporte de libalpm 15. Enlazar con libalpm significa recompilar Sanae cada vez que pacman sube de versión y romperse mientras tanto. El CLI de pacman y `expac` son estables desde hace una década. Sanae **no enlaza libalpm**: ejecuta pacman. Es más Unix, más KISS y es lo que Pacsea eligió también.

---

## 4. Arquitectura

**Lenguaje: Rust.** Motivos: binario único estático que se copia a la ISO o al chroot sin dependencias, rendimiento con listas de 15 000 paquetes, ratatui es hoy la librería TUI más usada (0.30, 22 000 estrellas, sin dependencias en C), y ya tienes Rust en ToyPOS. La alternativa seria sería Go con bubbletea (la misma familia que gum): igual de válida, algo menos rápida en listas grandes y con un ecosistema TUI menor. Se decide contigo (sección 10).

**Crates:** `ratatui` + `crossterm` (interfaz), `tokio` + `reqwest` (HTTP asíncrono para AUR, pkgstats, noticias, sin bloquear la interfaz), `serde` + `serde_json` + `quick-xml` (AUR, AppStream), `flate2` (XML comprimidos), `toml` (config y recetas), `nucleo` o `fuzzy-matcher` (búsqueda difusa instantánea), `portable-pty` (ejecutar pacman con salida en vivo y contraseña de sudo dentro de la TUI), `directories` (rutas XDG), `clap` (CLI). Nada más.

**Reutilizar arch-toolkit** (MIT) es tentador para el RPC del AUR y el parseo de `.SRCINFO`, pero es un proyecto joven de una sola persona con muchas funciones que no queremos. Recomendación: no depender de él; copiar la idea. Se decide contigo.

```
sanae
├── src/
│   ├── main.rs            CLI (clap) → TUI o subcomandos
│   ├── sources/
│   │   ├── pacman.rs      expac/pacman: catálogo, instalados, detalle, archivos, huérfanos
│   │   ├── aur.rs         RPC v5 (search, info por lotes), PKGBUILD, comentarios, caché
│   │   ├── appstream.rs   lectura de /usr/share/swcatalog: nombre, resumen, categorías, i18n
│   │   ├── pkgstats.rs    popularidad, caché de 24 h
│   │   └── news.rs        RSS de Arch
│   ├── index.rs           un solo índice en memoria: Package { name, repo|aur, version, summary, appstream?, popularity?, installed?, explicit? }
│   ├── queue.rs           cola de cambios: instalar / quitar / actualizar; preflight con `pacman -Sp` y `--print` (descargas, conflictos, espacio)
│   ├── exec.rs            ejecución en pty: sudo/doas + pacman o paru/yay, salida en vivo, cancelación
│   ├── recipes.rs         cargar, validar y aplicar recetas (idempotente, con --chroot)
│   ├── config.rs          ~/.config/sanae/config.toml y theme.toml
│   └── ui/
│       ├── app.rs         estado, teclas, pestañas
│       ├── store.rs       categorías, destacados, recetas
│       ├── search.rs      buscador unificado
│       ├── details.rs     pestañas de detalle
│       ├── installed.rs   instalados, huérfanos, explícitos
│       ├── updates.rs     actualizaciones + noticias
│       ├── queue.rs       cola y preflight
│       ├── run.rs         panel de ejecución
│       └── theme.rs       colores
├── recipes/               recetas incluidas (se instalan en /usr/share/sanae/recipes)
├── Cargo.toml
└── README.md
```

Estado en disco (todo borrable sin consecuencias): `~/.cache/sanae/` (AUR, pkgstats, noticias, metadatos), `~/.config/sanae/` (config, tema, recetas propias), `~/.local/state/sanae/history.log` (qué se instaló y cuándo, además del log de pacman que sigue siendo la verdad).

---

## 5. Interfaz

Una barra superior de pestañas y un panel principal. Todo se maneja con teclado; ratón opcional.

```
 早苗 Sanae   [1] Tienda  [2] Buscar  [3] Instalados  [4] Actualizaciones (7)  [5] Cola (3)  [6] Recetas          ? ayuda
┌ Categorías ──────────┐┌ Internet · 148 apps · ordenado por popularidad ─────────────────────────────────────┐
│ ▸ Destacados         ││   Firefox              Navegador web                     extra   69 %   ✔ instalado │
│   Internet           ││   Chromium             Navegador web de Google           extra   31 %               │
│   Multimedia         ││   qBittorrent          Cliente BitTorrent                extra   18 %               │
│   Oficina            ││ ▸ Telegram Desktop     Mensajería                        extra   15 %   ◉ en cola   │
│   Gráficos           ││   Thunderbird          Correo                            extra   14 %               │
│   Desarrollo         ││   Discord              Chat de voz y texto               extra   12 %               │
│   Juegos             ││   Brave                Navegador                         AUR      9 %               │
│   Sistema            │└──────────────────────────────────────────────────────────────────────────────────────┘
│   Ciencia            │┌ Telegram Desktop · telegram-desktop 6.2.1-1 · extra ────────────────────────────────┐
│   Educación          ││ Cliente oficial de Telegram para escritorio. Mensajes, llamadas, canales, archivos… │
│   Fuentes            ││ Licencia GPL-3.0 · 78 MB instalado · 12 dependencias · https://desktop.telegram.org │
│   Temas              ││ [Info] [Dependencias] [Archivos] [Capturas]                                         │
└──────────────────────┘└──────────────────────────────────────────────────────────────────────────────────────┘
 espacio marcar · enter detalle · / buscar · i instalar ahora · u actualizar todo · a aplicar cola · q salir
```

**Pestañas.**

- **Tienda.** Categorías de AppStream a la izquierda; a la derecha las apps de esa categoría con nombre humano, resumen en español, repo, popularidad y estado. "Destacados" = las más instaladas según pkgstats que no tienes. Las categorías "Fuentes" y "Temas" son recetas, no paquetes.
- **Buscar.** Una caja; busca al escribir en repos, AUR y AppStream a la vez (difusa, instantánea en lo local; el AUR llega medio segundo después y se funde en la lista). Filtros con una tecla: solo repos, solo AUR, solo instalados. Muestra el nombre técnico y el humano.
- **Detalle** (al pulsar Enter en cualquier sitio). Pestañas: Info (descripción, versión, licencia, tamaños, URL, mantenedor, votos y popularidad del AUR, fecha), Dependencias (requiere / requerido por / opcionales, con estado instalado o no), Archivos (`pacman -Ql`, o `pacman -Fl` si no está instalado), y para el AUR: PKGBUILD y Comentarios. Capturas: solo las URLs (una TUI no las muestra; se abren con `xdg-open` si hay escritorio).
- **Instalados.** Lista con filtros: explícitos, dependencias, huérfanos (`pacman -Qdt`), del AUR (`-Qm`), por repo, por tamaño. Acciones: marcar para quitar, marcar como explícito/dependencia (`pacman -D`), quitar huérfanos de golpe.
- **Actualizaciones.** Repos (`checkupdates`) y AUR (comparando versiones con el RPC). Encima, las noticias de Arch de las últimas semanas, marcadas si tienen "manual intervention". Un botón: actualizar todo, que es `pacman -Syu` y luego `paru -Sua`.
- **Cola.** Todo lo marcado. "Preflight" con `pacman -Sp` / `--print`: qué se descarga, cuánto pesa, conflictos, qué se quita. Se aplica con una tecla; la contraseña de sudo se pide dentro de la TUI.
- **Ejecución.** Panel a pantalla completa con la salida real de pacman/paru en vivo, barra de progreso por paquete (pacman imprime `(3/12)`), y al terminar un resumen. Cancelable con Ctrl+C (pacman deja el sistema consistente entre transacciones).
- **Recetas.** Lista de recetas con estado (aplicada / no aplicada / parcial), descripción de lo que hace cada una, y aplicar.

**Teclas** (fijas, sin capas de modos): `1-6` pestañas, `/` buscar, `↑↓ jk` moverse, `espacio` marcar, `enter` detalle, `i` instalar ahora, `d` quitar, `u` actualizar todo, `a` aplicar cola, `Tab` cambiar de panel, `?` ayuda, `q`/`Esc` atrás o salir. Todo es configurable en `keys.toml`, pero el objetivo es que no haga falta.

**Estética.** Paleta propia de Sanae en `theme.toml` (verde y azul del santuario Moriya, blanco y gris; el rojo se queda para Reimu), bordes redondeados, títulos en los bordes, iconos Nerd Font opcionales (se detectan y si no hay se usan `✔ ◉ ▸`). Nada parpadea; la única animación es el progreso.

**Idioma.** Inglés, como Reimu (decidido). Las descripciones de AppStream se muestran en inglés aunque el catálogo traiga traducciones; añadir español después es cambiar un archivo de cadenas.

---

## 6. Recetas

Una receta es un archivo TOML pequeño. Instala paquetes y deja la cosa funcionando. Es idempotente: aplicarla dos veces no rompe nada.

```toml
# /usr/share/sanae/recipes/qemu-kvm.toml
name = "Máquinas virtuales (QEMU/KVM)"
summary = "QEMU con KVM, libvirt y virt-manager, listo para crear una VM"
category = "Sistema"
packages = ["qemu-full", "libvirt", "virt-manager", "dnsmasq", "edk2-ovmf", "swtpm"]
services = ["libvirtd.service"]
groups = ["libvirt"]                      # el usuario se añade a estos grupos

[[files]]
path = "/etc/libvirt/network.conf"
append = 'firewall_backend = "iptables"'

[[commands]]
run = "virsh net-autostart default"
as = "root"
when = "always"                            # o "once"

check = "systemctl is-enabled libvirtd"    # cómo saber si ya está aplicada
notes = "Cierra sesión y vuelve a entrar para que el grupo libvirt haga efecto."
```

Campos: `packages` (repos), `aur` (AUR, requiere paru o yay), `services`, `groups`, `files` (`content` o `append`, con permisos), `commands` (`as = "root"|"user"`), `env` (líneas para `/etc/environment`), `needs` (otras recetas), `check`, `notes`. Nada de lógica: si una receta necesita un `if`, es que pide un script, y un script se puede llamar desde `commands`.

Recetas incluidas en la v1 (son las que hoy viven en Reimu como bundles y extras, más los temas): `fonts` (Noto, JetBrains Mono Nerd, Fira, Roboto, MS-compatibles y `fontconfig` con reglas por defecto), `japanese` (fcitx5 + mozc + variables de entorno), `qemu-kvm`, `docker`, `virtualbox`, `printing` (CUPS + PDF), `bluetooth`, `gaming` (Steam, Lutris, Wine, GameMode, MangoHud, multilib), `development` (git, editores, lenguajes), `office`, `multimedia`, `graphics`, `internet`, `utilities`, `firewall-ufw`, `firewall-firewalld`, `ssh-server`, `power-tlp`, `power-ppd`, `zram`, `snapshots-snapper`, `theme-xfce-*` (uno por tema: instala y escribe los XML de xfconf, exactamente lo que hoy hace Reimu), `flatpak` (solo si alguien lo pide).

`sanae apply --chroot /mnt --user jp qemu-kvm fonts` hace lo mismo dentro de un sistema recién instalado y sin arrancar: ejecuta cada comando con `arch-chroot`. Así Reimu no necesita saber nada de recetas.

---

## 7. Integración con Reimu y su simplificación

Hoy Reimu hace demasiado. Con Sanae, Reimu se queda con lo que **solo se puede hacer en la instalación** y Sanae con lo que se puede hacer **en cualquier momento**:

| Se queda en Reimu | Pasa a Sanae (como recetas) |
| --- | --- |
| Idioma, teclado, hora, hostname, mirrors | Bundles de software (development, internet, multimedia, graphics, gaming, utilities, fonts, japanese, virtualization) |
| Disco, sistema de archivos, LUKS, swap, snapshots | Temas e iconos de XFCE |
| Bootloader y kernels | Paquetes extra y servicios extra sueltos |
| Usuario, shell, sudo/doas, root | |
| Red, Bluetooth, impresión, firewall, SSH, gestión de energía (decidido: se quedan) | |
| Repositorios (multilib, Chaotic, propios) y ayudante de AUR | |
| Escritorio, gestor de sesión y driver de GPU | |
| Instalar Sanae y ejecutar las recetas elegidas en el chroot | |

Resultado: el asistente de Reimu pierde los ajustes de bundles, tema, paquetes extra y servicios extra (recorte parcial, decidido), las fases bundles y theme desaparecen, `catalog/` desaparece y en su lugar hay **una** pantalla al final: "Recetas de Sanae" con la misma lista y multiselección que verás luego dentro de Sanae. Reimu instala Sanae (paquete `sanae-bin` del AUR, o el binario del release si no hay ayudante de AUR) como última fase y llama a `sanae apply --chroot /mnt --user <usuario> <recetas>`. Si algo de una receta falla, falla dentro de Sanae, con su propio log, sin tirar la instalación; al primer arranque Sanae muestra "quedaron pendientes: …".

Además, Sanae al arrancar por primera vez en un sistema instalado por Reimu lee `/root/reimu.conf` y ofrece la misma lista de recetas por si el usuario quiere añadir algo después. Es la continuación natural.

Orden de trabajo: primero Sanae hasta el hito 4 (sección 9), después la integración, y solo entonces se recorta Reimu, para que en ningún momento Reimu pierda funciones sin que Sanae las tenga.

---

## 8. CLI (para scripts y para Reimu)

```
sanae                              abre la TUI
sanae search <texto> [--aur] [--json]
sanae info <paquete> [--json]
sanae install <paquetes…>          cola + preflight + aplicar, con confirmación
sanae remove <paquetes…>
sanae update [--news]              muestra las noticias, luego pacman -Syu y paru -Sua
sanae installed [--orphans|--explicit|--aur]
sanae store [categoría]            lista la categoría en texto
sanae recipes                      lista recetas y estado
sanae apply <recetas…> [--chroot DIR --user U] [--dry-run]
sanae clean                        caché de pacman y de Sanae
```

Salida legible por defecto, `--json` para máquinas, código de salida 0/1. `--dry-run` en `apply` imprime cada comando como hace Reimu.

---

## 9. Plan por hitos

| Hito | Contenido | Cómo se comprueba |
| --- | --- | --- |
| **M1 · Núcleo de datos** | `sources/pacman`, `sources/aur`, `index`, `sanae search/info --json` | Tests con salidas grabadas de expac y del RPC; búsqueda de 15 000 paquetes en < 50 ms |
| **M2 · TUI base** | Pestañas Buscar, Detalle, Instalados, Cola, Ejecución en pty con sudo | Instalar y quitar un paquete real en la VM desde la TUI |
| **M3 · Tienda** | AppStream, categorías, nombres e i18n, pkgstats, Destacados; Actualizaciones + noticias | Navegar la categoría Internet en español y actualizar la VM |
| **M4 · Recetas** | Formato, validación, `apply` con `--chroot` y `--dry-run`, las recetas incluidas | Aplicar `qemu-kvm` y `fonts` en la VM y en un chroot de Reimu |
| **M5 · Integración** | Reimu instala Sanae y delega; pantalla de recetas en Reimu; recorte de Reimu | Instalación completa desde la ISO con recetas elegidas |
| **M6 · Pulido** | Tema, iconos Nerd, `keys.toml`, README, `sanae-bin` en el AUR, CI con tests y clippy | Release 1.0 |

Cada hito se publica; M1 y M2 ya son útiles solos.

---

## 10. Decisiones tomadas (2026-09-10)

1. **Lenguaje:** Rust + ratatui.
2. **arch-toolkit:** no; el cliente del AUR es código propio.
3. **Flatpak:** fuera de la v1.
4. **Idioma de la interfaz:** solo inglés.
5. **Recorte de Reimu:** parcial. Solo bundles, temas y los extras sueltos pasan a Sanae; Bluetooth, impresión, firewall, SSH y energía siguen en Reimu.

---

## Fuentes consultadas

- [pacseek](https://github.com/moson-mo/pacseek) · [Pacsea](https://github.com/Firstp1ck/Pacsea) · [arch-toolkit](https://github.com/Firstp1ck/arch-toolkit) · [pamac](https://github.com/manjaro/pamac) · [Octopi](https://github.com/aarnt/octopi) · [AUR helpers (ArchWiki)](https://wiki.archlinux.org/title/AUR_helpers)
- [alpm.rs](https://github.com/archlinux/alpm.rs) y [crate alpm 5.0.2](https://crates.io/crates/alpm) · [pacman 7.1 en core (libalpm.so=16)](https://archlinux.org/packages/core/x86_64/pacman/)
- [ratatui 0.30](https://ratatui.rs/)
- AUR RPC v5 probado en vivo (`/rpc/v5/search`, `/rpc/v5/info`, `packages-meta-ext-v1.json.gz`)
- [archlinux-appstream-data](https://archlinux.org/packages/extra/any/archlinux-appstream-data/) · [pkgstats API](https://pkgstats.archlinux.de/api/packages/firefox) · [API JSON de paquetes de archlinux.org](https://wiki.archlinux.org/title/Official_repositories_web_interface)
