;@titre Le plus petit des deux (cours 6.1)
;@enonce Ce programme garde le plus GRAND des deux nombres. Modifie-le pour qu'il
;@enonce garde le plus petit — un seul saut conditionnel change.
;@enonce a vaut 17 et b vaut 42, donc RBX doit finir à 17.
;@attendu rbx == 17
;@attendu exit == 17

section .data
    a dq 17
    b dq 42

section .text
    global _start

_start:
    mov rsi, a                  ; l'ADRESSE de a…
    mov rbx, [rsi]              ; …et les crochets vont y chercher la VALEUR
    mov rsi, b
    mov rcx, [rsi]

    cmp rbx, rcx
    ; TODO : « jge » garde le plus grand. Quel saut garde le plus petit ?
    jge .fait                   ; RBX convient déjà
    mov rbx, rcx                ; sinon c'est RCX qu'on veut

.fait:
    mov rax, 60
    mov rdi, rbx
    syscall
