;@titre Affiche des étoiles (cours 7.2)
;@enonce Affiche 5 étoiles suivies d'un retour à la ligne, dans la boîte
;@enonce « Sortie du programme » (bouton ▣ de l'en-tête de la console).
;@enonce Le piège du chapitre : « syscall » écrase RAX, RCX et R11. Un compteur
;@enonce de boucle rangé dans RCX serait détruit à chaque affichage — d'où R12,
;@enonce que le noyau laisse tranquille.
;@enonce R14 doit compter les 5 étoiles écrites. Essaie ensuite avec 12.
;@attendu r14 == 5
;@attendu exit == 0

section .data
    etoile db "*"
    saut   db 10

section .text
    global _start

_start:
    mov r12, 5                  ; combien d'étoiles : R12 survit à syscall
    xor r14, r14                ; combien on en a réellement écrites

.boucle:
    mov rax, 1                  ; write
    mov rdi, 1                  ; stdout
    mov rsi, etoile
    mov rdx, 1                  ; un seul octet
    syscall
    ; TODO : compter l'étoile qu'on vient d'écrire  (« inc r14 »)

    dec r12
    jnz .boucle

    ; le retour à la ligne, une fois toutes les étoiles sorties
    mov rax, 1
    mov rdi, 1
    mov rsi, saut
    mov rdx, 1
    syscall

    mov rax, 60
    xor rdi, rdi
    syscall
