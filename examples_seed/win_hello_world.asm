; Windows PE64 — Bonjour
; Cible : Windows — PE64 console. Le programme appelle les fonctions de
; kernel32.dll : Windows n'utilise pas l'instruction syscall de Linux.

bits 64
default rel

section .data
    message     db "Bonjour depuis Windows PE64 !", 13, 10
    message_len equ $ - message

section .bss
    written     resd 1

section .text
    global main
    extern GetStdHandle
    extern WriteFile
    extern ExitProcess

main:
    sub     rsp, 40             ; 32 octets d'espace d'ombre + alignement

    mov     ecx, -11            ; STD_OUTPUT_HANDLE
    call    GetStdHandle        ; RAX = poignée de la console

    mov     rcx, rax            ; 1er argument : poignée
    lea     rdx, [message]      ; 2e : texte
    mov     r8d, message_len    ; 3e : nombre d'octets
    lea     r9, [written]       ; 4e : octets effectivement écrits
    mov     qword [rsp + 32], 0 ; 5e argument : OVERLAPPED = NULL
    call    WriteFile

    xor     ecx, ecx            ; code de sortie 0
    call    ExitProcess
