; Windows PE64 — Lire / écrire
; Tapez un caractère puis Entrée : ReadFile le lit, WriteFile le réaffiche.
; Dans ASM Studio, lancez-le avec la cible Windows — PE64 console.

bits 64
default rel

section .data
    prompt      db "Tapez un caractere puis Entree : ", 0
    prompt_len  equ $ - prompt - 1

section .bss
    buffer      resb 1
    read_count  resd 1
    written     resd 1

section .text
    global main
    extern GetStdHandle
    extern WriteFile
    extern ReadFile
    extern ExitProcess

main:
    sub     rsp, 40

    ; Afficher l'invite sur STD_OUTPUT_HANDLE.
    mov     ecx, -11
    call    GetStdHandle
    mov     r12, rax
    mov     rcx, r12
    lea     rdx, [prompt]
    mov     r8d, prompt_len
    lea     r9, [written]
    mov     qword [rsp + 32], 0
    call    WriteFile

    ; Lire un seul octet depuis STD_INPUT_HANDLE.
    mov     ecx, -10
    call    GetStdHandle
    mov     rcx, rax
    lea     rdx, [buffer]
    mov     r8d, 1
    lea     r9, [read_count]
    mov     qword [rsp + 32], 0
    call    ReadFile

    cmp     dword [read_count], 0
    je      .done               ; EOF : rien à réafficher

    mov     rcx, r12
    lea     rdx, [buffer]
    mov     r8d, 1
    lea     r9, [written]
    mov     qword [rsp + 32], 0
    call    WriteFile

.done:
    xor     ecx, ecx
    call    ExitProcess
