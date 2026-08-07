# ASM Studio

[English](README.md) | [Français](README.fr.md) | **Español**

> IDE pedagógico para aprender ensamblador **NASM x86-64** en Linux.

ASM Studio no es un simulador: su programa se **ensambla realmente**
(`nasm`), se **enlaza** (`ld`) y lo **ejecuta el núcleo Linux de verdad**,
dirigido paso a paso mediante `ptrace`. Lo que usted ve — registros,
banderas, pila, memoria — es el estado auténtico del proceso, no una
aproximación.

![Vista previa de ASM Studio](src/Assets/mockup-asm_studio.png)

---

## Contenido

- [Características](#características)
- [Instalación](#instalación)
- [Primeros pasos](#primeros-pasos)
- [Atajos de teclado](#atajos-de-teclado)
- [Compilar desde las fuentes](#compilar-desde-las-fuentes)
- [Dependencias](#dependencias)
- [Estructura del proyecto](#estructura-del-proyecto)
- [Licencia](#licencia)

---

## Características

- **Depurador real** — ejecución paso a paso de un binario NASM mediante
  `ptrace` (registros, `SETREGS`, lectura/escritura de `/proc/pid/mem`), no
  una máquina virtual simulada.
- **Puntos de interrupción** — un clic en el margen (o `Ctrl+F8`) marca una
  línea, y `Continuar` (`F9`) llega hasta ella de una vez. `Paso por encima`
  (`Mayús+F10`) ejecuta una `call` entera de golpe. Cada instrucción sigue
  entrando en la línea de tiempo.
- **Consola de verdad** — lo que el programa escribe en su salida estándar
  llega al IDE, y usted puede enviarle entrada: un programa detenido en un
  `read` le espera en lugar de congelar la interfaz.
- **Dos modos de visualización** — *Aprendizaje* (lo esencial: código,
  instrucción explicada, registros generales, pila, consola) y *Completo*
  (todo: desensamblado, vista de memoria, volcado hexadecimal, pila de
  llamadas, llamadas al sistema).
- **Disposición acoplable** — cada panel se arrastra, se apila o se desprende
  en una ventana flotante (`egui_dock`), como en un IDE clásico.
- **Recorrido guiado** — un tutorial de cuatro niveles (Principiante,
  Intermedio, Avanzado, Experto) que introduce progresivamente registros,
  tamaños, memoria, banderas, pila y llamadas al sistema.
- **Ejercicios autocorregidos** — una veintena de ejercicios con verificación
  automática del resultado.
- **Modo «CPU viva»** — animación de los registros y banderas modificados en
  cada paso, con distintivos de actividad PUSH/POP sobre la pila.
- **Predicción** — adivine el efecto de la próxima instrucción antes de
  ejecutarla, para afianzar la comprensión.
- **Calculadora integrada** — hexadecimal por defecto, vista bit a bit,
  operaciones habituales.
- **Diagnóstico de errores** — los mensajes de `nasm`/`ld` y los fallos en
  ejecución se reformulan en lenguaje claro.
- **Multilingüe** — interfaz en francés, inglés y español.
- **Actualización automática** — comprobación de nuevas versiones mediante
  GitHub Releases.

---

## Instalación

### Binario precompilado

Descargue el último archivo desde las
[Releases de GitHub](https://github.com/fredza/asm-studio/releases), y luego:

```bash
tar xzf asm-studio-*-linux-x86_64.tar.gz
cd asm-studio-*/
./install.sh                  # instalación de usuario, en ~/.local
# o bien
sudo ./install.sh --system    # instalación de sistema, en /usr/local
```

El script instala el binario, el icono y el archivo `.desktop`, y comprueba
que `nasm` y `ld` estén presentes.

### Véase también

- [`DEPENDENCIES.md`](DEPENDENCIES.md) — lista completa de las bibliotecas de
  sistema necesarias (Wayland/X11, portal XDG, `nasm`, `binutils`…) y los
  comandos de instalación por distribución.
- [`doc/GUIDE-DEMARRAGE-RAPIDE.md`](doc/GUIDE-DEMARRAGE-RAPIDE.md) — guía de
  uso completa (por ahora solo en francés): primer programa, paneles, atajos,
  resolución de problemas.

---

## Primeros pasos

En el primer arranque, ASM Studio crea sus carpetas de trabajo, las siembra
con ejemplos y ejercicios comentados, y abre en **modo Aprendizaje** con un
cartel que propone empezar el tutorial guiado.

Un primer programa mínimo (`Archivo → Nuevo`, `Ctrl+N`):

```nasm
section .text
    global _start
_start:
    mov rax, 60      ; sys_exit
    xor rdi, rdi     ; código de salida 0
    syscall
```

Ciclo de trabajo: **Ensamblar → Ejecutar → Paso a paso → Línea de tiempo**.
Todos los detalles en la
[guía de inicio rápido](doc/GUIDE-DEMARRAGE-RAPIDE.md) (en francés).

---

## Atajos de teclado

Los principales; `F1` muestra la lista completa dentro de la aplicación.

| Tecla | Acción |
|---|---|
| `F1` | Mostrar / ocultar la ayuda de atajos |
| `Ctrl+B` | Ensamblar y enlazar |
| `F5` | Ejecutar / reiniciar |
| `F10` (o `F8`) | Instrucción siguiente |
| `Mayús+F10` | Paso por encima: ejecuta la llamada de una vez |
| `F9` | Continuar hasta el próximo punto de interrupción |
| `Ctrl+F8` | Punto de interrupción en la línea del cursor (o clic en el margen) |
| `Esc` (o `Mayús+F5`) | Detener el programa |
| `←` / `→` | Línea de tiempo: paso anterior / siguiente |
| `Inicio` / `Fin` | Línea de tiempo: inicio / fin |
| `Ctrl+N` / `Ctrl+O` / `Ctrl+S` | Nuevo / Abrir / Guardar |
| `Ctrl+F` / `Ctrl+H` | Buscar / buscar y reemplazar |
| `Ctrl+Mayús+P` | Paleta de comandos — toda la aplicación desde el teclado |
| `Ctrl+1` … `Ctrl+5` | Mostrar / ocultar un panel |
| `F6` / `Mayús+F6` | Panel siguiente / anterior |

Toda la interfaz se maneja con el teclado: la paleta de comandos
(`Ctrl+Mayús+P`) alcanza cualquier acción sin pasar por los menús.

---

## Compilar desde las fuentes

Requisitos: Rust (edición 2024), `nasm`, `binutils` (`ld`) y las bibliotecas
listadas en [`DEPENDENCIES.md`](DEPENDENCIES.md) (Wayland/EGL, `libxkbcommon`,
portal XDG).

```bash
git clone https://github.com/fredza/asm-studio.git
cd asm-studio
cargo build --release
./target/release/asm_studio
```

Generar un archivo de distribución (binario + recursos + scripts):

```bash
./install/package.sh
# → dist/asm-studio-<versión>-linux-x86_64.tar.gz
```

Ejecutar las pruebas:

```bash
cargo test
```

---

## Dependencias

Plataforma: **Linux x86-64** únicamente (la ejecución paso a paso se apoya en
`ptrace`). Principales dependencias Rust:

| Crate | Función |
|---|---|
| `eframe` / `egui_dock` | interfaz gráfica y disposición acoplable |
| `nix` | `ptrace`, gestión de procesos y señales |
| `capstone` | desensamblado x86-64 |
| `object` | lectura de archivos ELF |
| `rfd` | diálogos de archivo nativos (portal XDG) |
| `ureq` / `serde` | comprobación de actualizaciones (GitHub Releases) |

Herramientas externas necesarias en ejecución: `nasm` (ensamblador) y `ld`
(enlazador). Véase [`DEPENDENCIES.md`](DEPENDENCIES.md) para el detalle
completo (bibliotecas de sistema, paquetes por distribución, comprobación
rápida).

---

## Estructura del proyecto

```
src/
├── app/            interfaz (paneles acoplables, menús, atajos, tutorial…)
├── debugger.rs      control por ptrace del proceso depurado
├── disasm.rs         desensamblado (capstone)
├── assemble.rs       invocación de nasm / ld
├── tutorial.rs        contenido del recorrido guiado
├── exercise.rs         ejercicios autocorregidos
├── i18n.rs              traducciones FR / EN / ES
└── main.rs                punto de entrada
examples_seed/      ejemplos y ejercicios sembrados en el primer arranque
install/            scripts de instalación y empaquetado
doc/                guía de inicio rápido
```

---

## Licencia

Distribuido bajo la **ASM Studio Personal Free License (ASFL) v1.0** — véase
[`LICENSE.md`](LICENSE.md). En resumen: uso libre y gratuito, código fuente
consultable y modificable, contribuciones por *pull request* bienvenidas; la
venta o la redistribución comercial del software (original o modificado) está
prohibida sin autorización escrita del autor.

Copyright © 2026 Frédéric Zawalski.
