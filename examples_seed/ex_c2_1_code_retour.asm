;@titre Change le résultat (cours 2.1)
;@enonce Ce programme renvoie 42. Fais-lui renvoyer 7, puis 0 — le code qui
;@enonce signifie traditionnellement « tout s'est bien passé ».
;@enonce Essaie ensuite 256 : le code de sortie n'en garde que les 8 bits du bas,
;@enonce et tu liras 0. Avec 300, tu liras 44. Laisse 7 pour valider.
;@attendu exit == 7

section .text
    global _start

_start:
    mov rax, 60                 ; syscall exit
    mov rdi, 42                 ; TODO : le code de sortie voulu
    syscall
