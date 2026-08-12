; Windows PE64 — Boucle : affiche les chiffres 1 à 9
; Démontre : inc, cmp, jne et WriteFile dans une boucle.

bits 64
default rel

section .data
    digit       db '0', 13, 10

section .bss
    written     resd 1

section .text
    global main
    extern GetStdHandle
    extern WriteFile
    extern ExitProcess

main:
    sub     rsp, 40

    mov     ecx, -11
    call    GetStdHandle
    mov     r12, rax            ; la poignée reste la même à chaque tour
    mov     ebx, 1

.loop:
    mov     eax, ebx
    add     al, '0'
    mov     [digit], al

    mov     rcx, r12
    lea     rdx, [digit]
    mov     r8d, 3              ; chiffre + CRLF
    lea     r9, [written]
    mov     qword [rsp + 32], 0
    call    WriteFile

    inc     ebx
    cmp     ebx, 10
    jne     .loop

    xor     ecx, ecx
    call    ExitProcess
