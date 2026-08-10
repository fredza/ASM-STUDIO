;@titre Additionner deux nombres saisis (cours 10.2)
;@enonce Demande deux nombres l'un après l'autre, puis affiche leur somme.
;@enonce La lecture et la conversion sont déjà là, rangées dans « lire_nombre » :
;@enonce il te reste à l'appeler deux fois et à additionner les deux résultats.
;@enonce Tape tes nombres dans la ligne de saisie de la console, et lis la somme
;@enonce dans la boîte « Sortie du programme ».
;@enonce Le contrôle vérifie la terminaison propre et l'usage de R15, le second
;@enonce nombre : ce que tu saisiras, lui, il ne peut pas le deviner.
;@requis r15
;@attendu exit == 0

section .bss
    saisie       resb 16
    texte_nombre resb 16

section .data
    invite1     db "Premier nombre : "
    invite1_len equ $ - invite1
    invite2     db "Deuxieme nombre : "
    invite2_len equ $ - invite2
    total       db "Somme : "
    total_len   equ $ - total
    saut        db 10

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

; lire_nombre() → RAX : lit une ligne au clavier et la convertit en entier.
; La conversion s'arrête au premier octet qui n'est pas un chiffre — le retour à
; la ligne, en général. Se fier au nombre d'octets lus serait plus court, mais
; une ligne vide ou une fin d'entrée donnerait un compteur négatif, et la boucle
; ne s'arrêterait jamais.
lire_nombre:
    mov rax, 0                  ; read
    mov rdi, 0                  ; stdin
    mov rsi, saisie
    mov rdx, 16
    syscall

    mov rcx, rax                ; octets lus, ou 0 en fin d'entrée
    mov rsi, saisie
    xor rax, rax
.convertir:
    cmp rcx, 0
    jle .fini                   ; plus rien à lire
    movzx rbx, byte [rsi]
    cmp bl, '0'
    jb .fini                    ; ce n'est plus un chiffre
    cmp bl, '9'
    ja .fini
    sub rbx, '0'
    imul rax, rax, 10           ; chaque chiffre décale le total d'un rang
    add rax, rbx
    inc rsi
    dec rcx
    jmp .convertir
.fini:
    ret

_start:
    mov rdi, invite1
    mov rsi, invite1_len
    call afficher
    call lire_nombre
    mov r14, rax                ; R14 survit aux syscalls qui suivent

    mov rdi, invite2
    mov rsi, invite2_len
    call afficher
    ; TODO : lire le deuxième nombre et le garder dans R15

    mov rdi, total
    mov rsi, total_len
    call afficher

    ; TODO : additionner R14 et R15, et mettre le total dans RDI
    mov rdi, r14
    call afficher_nombre

    mov rax, 60
    xor rdi, rdi
    syscall
