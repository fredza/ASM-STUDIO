; Windows PE64 — Arithmétique : add, sub, imul, idiv
; Le calcul est identique à Linux ; seules l'affichage et la sortie utilisent
; l'API Windows (convention RCX, RDX, R8, R9).

bits 64
default rel

section .data
    result      db "Resultat = 0", 13, 10
    result_len  equ $ - result

section .bss
    written     resd 1

section .text
    global main
    extern GetStdHandle
    extern WriteFile
    extern ExitProcess

main:
    sub     rsp, 40

    mov     rax, 10             ; 10
    add     rax, 5              ; 15
    sub     rax, 3              ; 12
    imul    rax, 4              ; 48
    xor     rdx, rdx            ; dividende RDX:RAX = 48
    mov     rcx, 6
    idiv    rcx                 ; RAX = 8, RDX = 0
    add     al, '0'             ; 8 devient le caractère ASCII '8'
    mov     [result + 11], al

    mov     ecx, -11
    call    GetStdHandle
    mov     rcx, rax
    lea     rdx, [result]
    mov     r8d, result_len
    lea     r9, [written]
    mov     qword [rsp + 32], 0
    call    WriteFile

    xor     ecx, ecx
    call    ExitProcess
