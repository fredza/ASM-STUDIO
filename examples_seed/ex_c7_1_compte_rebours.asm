;@titre Compte à rebours (cours 7.1)
;@enonce Calcule la somme de 10 à 1, dans ce sens-là. Le résultat est le même que
;@enonce celui de la somme de 1 à 10 : pourquoi ? Parce que l'addition se moque
;@enonce de l'ordre des termes — mais la boucle, elle, n'est pas la même.
;@enonce RBX doit valoir 55. Modifie ensuite les bornes pour aller de 1 à 20 :
;@enonce tu dois trouver 210.
;@attendu rbx == 55
;@attendu exit == 55

section .text
    global _start

_start:
    xor rbx, rbx                ; la somme
    mov rcx, 10                 ; on part du haut

.boucle:
    ; TODO : ajouter le compteur à la somme  (« add rbx, rcx »)
    dec rcx
    jnz .boucle                 ; on s'arrête quand RCX tombe à zéro

    mov rax, 60
    mov rdi, rbx
    syscall
