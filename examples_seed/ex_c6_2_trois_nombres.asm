;@titre Le plus grand de trois (cours 6.2)
;@enonce Trois nombres en mémoire, un seul gagnant. Indice du cours : compare
;@enonce d'abord a et b, garde le plus grand dans un registre, puis compare ce
;@enonce résultat avec c.
;@enonce a = 17, b = 42, c = 25 : RBX doit finir à 42.
;@attendu rbx == 42
;@attendu exit == 42

section .data
    a dq 17
    b dq 42
    c dq 25

section .text
    global _start

_start:
    mov rsi, a                  ; l'ADRESSE de a…
    mov rbx, [rsi]              ; …et les crochets vont y chercher la VALEUR
                                ; RBX = le meilleur connu pour l'instant

    mov rsi, b
    mov rcx, [rsi]
    cmp rbx, rcx
    jge .contre_c               ; RBX tient toujours
    mov rbx, rcx                ; b prend la tête

.contre_c:
    mov rsi, c
    mov rcx, [rsi]
    ; TODO : même schéma avec c — comparer, sauter si RBX gagne encore,
    ; TODO : sinon prendre RCX.

.fait:
    mov rax, 60
    mov rdi, rbx
    syscall
