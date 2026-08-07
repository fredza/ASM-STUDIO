; Démo de la pile — push/pop, appel de fonction, cadre de pile
; Observer RSP et les valeurs dans le panneau Pile du débogueur

section .text
    global _start

_start:
    ; empiler quelques valeurs
    push 100
    push 200
    push 300

    ; appeler une fonction (crée un cadre de pile)
    call additionne
    ; rax = 100 + 200 + 300 = 600

    ; exit(rax & 0xFF)
    mov rdi, rax
    and rdi, 0xFF
    mov rax, 60
    syscall

; Additionne les trois valeurs au sommet de la pile
; Entrée : [rsp+8] = 300, [rsp+16] = 200, [rsp+24] = 100
; Sortie  : rax = somme
additionne:
    push rbp
    mov  rbp, rsp

    mov rax, [rbp+16]   ; 300
    add rax, [rbp+24]   ; + 200
    add rax, [rbp+32]   ; + 100

    pop rbp
    ret
