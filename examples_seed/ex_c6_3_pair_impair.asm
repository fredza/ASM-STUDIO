;@titre Pair ou impair (cours 6.3)
;@enonce Un nombre est pair si son bit du bas vaut zéro. « test rbx, 1 » agit comme
;@enonce « cmp », mais avec un ET logique au lieu d'une soustraction : le drapeau
;@enonce zéro s'arme quand RBX ET 1 donne 0 — c'est-à-dire quand RBX est pair.
;@enonce Renvoie 1 si le nombre est pair, 0 s'il est impair. Ici 34 : donc 1.
;@requis test
;@attendu rcx == 1
;@attendu exit == 1

section .text
    global _start

_start:
    mov rbx, 34                 ; le nombre à examiner
    xor rcx, rcx                ; la réponse : 0 par défaut

    ; TODO : tester le bit du bas de RBX  (« test rbx, 1 »)
    ; TODO : sauter à .fait s'il n'est pas nul  (« jnz .fait » : impair)
    ; TODO : sinon mettre RCX à 1

.fait:
    mov rax, 60
    mov rdi, rcx
    syscall
