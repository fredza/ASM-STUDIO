;@titre Manipuler les tailles (cours 3.1)
;@enonce RBX contient 0xCAFE. Renvoie uniquement ses 8 bits du bas comme code de
;@enonce sortie : BL, la moitié basse de BX, elle-même moitié basse de EBX.
;@enonce Calcule le résultat à la main AVANT de lancer — 0xCAFE se termine par 0xFE.
;@enonce « movzx » (move with zero extension) recopie un petit registre dans un
;@enonce grand en comblant le haut de zéros ; c'est lui qu'on attend ici.
;@requis movzx
;@attendu exit == 254

section .text
    global _start

_start:
    mov rbx, 0xCAFE

    ; TODO : mettre dans RDI les 8 bits du bas de RBX, et rien d'autre.
    mov rdi, 0

    mov rax, 60
    syscall
