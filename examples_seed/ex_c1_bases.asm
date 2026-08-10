;@titre Binaire, décimal, hexadécimal (cours 1.1 à 1.3)
;@enonce Réponds aux trois exercices du chapitre 1 en écrivant chaque réponse
;@enonce dans le registre indiqué — et dans la BASE demandée, pas une autre :
;@enonce c'est la notation qu'on t'apprend à reconnaître.
;@enonce RBX = 5 en binaire, RCX = 13 en binaire, RDX = 20 en binaire (0b…),
;@enonce RSI = 1100 (binaire) en décimal, R8 = 10000 (binaire) en décimal,
;@enonce R9 = 0x1F en décimal, R10 = 16 en hexadécimal (0x…),
;@enonce R11 = 1010 1100 (binaire) en hexadécimal, sans passer par le décimal.
;@attendu rbx == 5
;@attendu rcx == 13
;@attendu rdx == 20
;@attendu rsi == 12
;@attendu r8 == 16
;@attendu r9 == 31
;@attendu r10 == 16
;@attendu r11 == 172
;@attendu exit == 0

section .text
    global _start

_start:
    ; 1.1 — décimal vers binaire. Écris la valeur en notation 0b… (indice : 5 = 4 + 1)
    mov rbx, 0b0                ; TODO : 5
    mov rcx, 0b0                ; TODO : 13
    mov rdx, 0b0                ; TODO : 20

    ; 1.2 — binaire vers décimal. Écris la valeur en décimal ordinaire.
    mov rsi, 0                  ; TODO : que vaut 1100 ?
    mov r8, 0                   ; TODO : que vaut 10000 ?

    ; 1.3 — hexadécimal.
    mov r9, 0                   ; TODO : 0x1F en décimal
    mov r10, 0x0                 ; TODO : 16 en hexadécimal (notation 0x…)
    mov r11, 0x0                 ; TODO : 1010 1100 en hexa — un quartet = un chiffre

    mov rax, 60
    xor rdi, rdi
    syscall
