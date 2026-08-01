;@titre Compter les bits à 1
;@enonce Compte combien de bits valent 1 dans 0xB7 (= 1011 0111), et laisse
;@enonce ce nombre — 6 — dans RBX.
;@attendu rbx == 6
;@attendu exit == 0

section .text
    global _start

_start:
    mov rax, 0xB7       ; la valeur à examiner
    xor rbx, rbx        ; compteur de bits à 1
    mov rcx, 8          ; huit bits à parcourir

.boucle:
    shr rax, 1          ; fait sortir le bit de poids faible dans la retenue (CF)
    ; TODO : ajouter la retenue à RBX  (« adc rbx, 0 » : add with carry)
    dec rcx
    jnz .boucle

    mov rax, 60
    xor rdi, rdi
    syscall
