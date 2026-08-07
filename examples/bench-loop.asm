; bench-loop.asm — 40 000 instructions et rien d'autre.
;
; Sert la mesure `bench_forty_thousand_steps` (src/app/debug_ops.rs) : une
; boucle serrée, sans appel système, pour chiffrer le coût d'un pas et
; vérifier que « Continuer » tient la charge.

section .text
    global _start
_start:
    mov rcx, 20000
.loop:
    dec rcx
    jnz .loop
    mov rax, 60
    xor rdi, rdi
    syscall
