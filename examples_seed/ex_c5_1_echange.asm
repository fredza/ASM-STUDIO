;@titre Échanger sans la pile (cours 5.1)
;@enonce pile_demo.asm échange deux registres en passant par la pile. Obtiens le
;@enonce même résultat SANS la pile, avec un troisième registre temporaire.
;@enonce RBX vaut 10 et RCX vaut 20 : après l'échange, RBX doit valoir 20 et RCX 10.
;@enonce Question du cours à méditer ensuite : que se passe-t-il si on inverse
;@enonce l'ordre des deux « pop » dans pile_demo.asm ? Prédis, puis vérifie.
;@interdit push
;@interdit pop
;@attendu rbx == 20
;@attendu rcx == 10
;@attendu exit == 0

section .text
    global _start

_start:
    mov rbx, 10
    mov rcx, 20

    ; TODO : échanger RBX et RCX en passant par RDX.
    ; TODO : trois « mov » suffisent — mais dans quel ordre ? Écrase l'un des
    ; TODO : deux trop tôt, et sa valeur est perdue pour de bon.

    mov rax, 60
    xor rdi, rdi
    syscall
