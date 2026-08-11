; simd.asm — calcul flottant et vectoriel, pour le panneau SSE / FPU
;
; Deux façons de se servir des XMM, dans l'ordre où on les rencontre :
;   1. scalaire — un seul double dans les 64 bits bas (addsd, mulsd) ;
;   2. vectoriel — quatre entiers additionnés d'un seul coup (paddd).
;
; Rien ne s'affiche : tout se lit dans le panneau SSE / FPU, en changeant la
; vue (« 2 × f64 » pour la partie scalaire, « 4 × i32 » pour la partie SIMD).

section .data
    align 16
    deux    dq 2.0
    trois   dq 3.0
    a       dd 1, 2, 3, 4          ; quatre entiers de 32 bits
    b       dd 10, 20, 30, 40

section .text
    global _start

_start:
    ; --- scalaire : xmm0 = 2.0 + 3.0, puis × 2.0 ---
    movsd   xmm0, [rel deux]       ; xmm0 = 2.0 (vue « 2 × f64 » : 2 │ 0)
    movsd   xmm1, [rel trois]      ; xmm1 = 3.0
    addsd   xmm0, xmm1             ; xmm0 = 5.0
    mulsd   xmm0, [rel deux]       ; xmm0 = 10.0

    ; --- vectoriel : quatre additions en une instruction ---
    movdqa  xmm2, [rel a]          ; xmm2 = 1 │ 2 │ 3 │ 4   (vue « 4 × i32 »)
    movdqa  xmm3, [rel b]          ; xmm3 = 10 │ 20 │ 30 │ 40
    paddd   xmm2, xmm3             ; xmm2 = 11 │ 22 │ 33 │ 44

    ; --- redescendre un résultat dans un registre général ---
    cvtsd2si rax, xmm0             ; rax = 10 (le double converti en entier)
    movq    rbx, xmm2              ; rbx = les DEUX entiers bas (11 et 22)

    mov     rax, 60                ; sys_exit
    xor     rdi, rdi
    syscall
