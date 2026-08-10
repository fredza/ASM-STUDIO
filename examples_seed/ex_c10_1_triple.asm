;@titre Le triple au lieu du double (cours 10.1)
;@enonce Ce programme lit un nombre au clavier et affiche son double. Fais-lui
;@enonce afficher le triple.
;@enonce Tape ton nombre dans la ligne de saisie de la console, puis lis le
;@enonce résultat dans la boîte « Sortie du programme ».
;@enonce Le contrôle ne peut pas deviner ce que tu saisiras : il vérifie que le
;@enonce programme se termine proprement, et que le facteur 3 figure bien dans
;@enonce le code — le résultat, c'est à toi de le lire.
;@requis 3
;@attendu exit == 0

section .bss
    saisie       resb 16
    texte_nombre resb 16

section .data
    invite     db "Un nombre entier positif : "
    invite_len equ $ - invite
    saut       db 10

section .text
    global _start

; afficher(RDI = adresse, RSI = longueur)
afficher:
    mov rdx, rsi
    mov rsi, rdi
    mov rax, 1
    mov rdi, 1
    syscall
    ret

; afficher_nombre(RDI = nombre), suivi d'un retour à la ligne
afficher_nombre:
    mov rax, rdi
    mov r8, texte_nombre + 16
    xor rcx, rcx
.chiffre:
    dec r8
    xor rdx, rdx
    mov r9, 10
    div r9
    add rdx, '0'
    mov [r8], dl
    inc rcx
    cmp rax, 0
    jnz .chiffre
    mov rdi, r8
    mov rsi, rcx
    call afficher
    mov rdi, saut
    mov rsi, 1
    call afficher
    ret

_start:
    mov rdi, invite
    mov rsi, invite_len
    call afficher

    mov rax, 0                  ; syscall read
    mov rdi, 0                  ; stdin
    mov rsi, saisie
    mov rdx, 16
    syscall

    ; Le texte saisi devient un nombre : chaque chiffre vaut dix fois le total
    ; précédent, plus lui-même. On s'arrête au premier octet qui n'est pas un
    ; chiffre — le retour à la ligne.
    mov rcx, rax                ; octets lus, ou 0 en fin d'entrée
    mov rsi, saisie
    xor rax, rax

.convertir:
    cmp rcx, 0
    jle .calcul                 ; plus rien à lire
    movzx rbx, byte [rsi]
    cmp bl, '0'
    jb .calcul                  ; ce n'est plus un chiffre
    cmp bl, '9'
    ja .calcul
    sub rbx, '0'
    imul rax, rax, 10
    add rax, rbx
    inc rsi
    dec rcx
    jmp .convertir

.calcul:
    ; TODO : multiplier RAX par 3 au lieu de 2  (« imul rax, rax, 2 » → ?)
    imul rax, rax, 2

    mov rdi, rax
    call afficher_nombre

    mov rax, 60
    xor rdi, rdi
    syscall
