;@titre Affiche ton propre message (cours 2.2)
;@enonce Change le texte de msg1 pour un message de ton choix, puis complète le
;@enonce second bloc write pour afficher msg2 à la suite.
;@enonce Le résultat se lit dans la boîte « Sortie du programme » (bouton ▣ de
;@enonce l'en-tête de la console) : c'est ce qu'un terminal afficherait.
;@enonce R14 doit valoir 2 : deux messages écrits.
;@attendu r14 == 2
;@attendu exit == 0

section .data
    msg1     db "Salut, je suis ton premier programme en assembleur !", 10
    msg1_len equ $ - msg1       ; $ = ici ; la différence donne la longueur
    msg2     db "Et voici ma deuxieme ligne.", 10
    msg2_len equ $ - msg2

section .text
    global _start

_start:
    xor r14, r14                ; compteur de messages écrits

    mov rax, 1                  ; syscall write
    mov rdi, 1                  ; descripteur 1 = stdout
    mov rsi, msg1               ; adresse du texte
    mov rdx, msg1_len           ; sa longueur
    syscall
    inc r14

    ; TODO : le même bloc pour msg2 (rax=1, rdi=1, rsi=msg2, rdx=msg2_len, syscall)
    ; TODO : puis « inc r14 »

    mov rax, 60
    xor rdi, rdi
    syscall
