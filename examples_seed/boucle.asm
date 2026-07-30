; Boucle — compte de 1 à 10, affiche chaque chiffre
; Démontre : dec, jnz, write

section .data
    chiffre db "0", 10          ; caractère + newline

section .text
    global _start

_start:
    mov rcx, 10                 ; compteur : 10 itérations

.boucle:
    ; transformer le compteur en caractère ASCII ('1'..'A')
    mov rax, rcx
    neg rax
    add rax, 11                 ; rax = 11 - rcx  (10→1, 9→2, …, 1→10)
    add rax, '0'                ; convertir en ASCII
    mov [chiffre], al

    ; write(1, chiffre, 2)
    mov rax, 1
    mov rdi, 1
    mov rsi, chiffre
    mov rdx, 2
    syscall

    dec rcx
    jnz .boucle                 ; répéter si rcx != 0

    ; exit(0)
    mov rax, 60
    xor rdi, rdi
    syscall
