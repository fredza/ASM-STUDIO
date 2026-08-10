;@titre FizzBuzz personnalisé (cours 11.3)
;@enonce Ajoute la troisième règle du cours : les multiples de 7 affichent « Bang ».
;@enonce Un nombre multiple de 3, 5 et 7 à la fois affiche les trois enchaînés.
;@enonce Regarde le résultat dans la boîte « Sortie du programme ».
;@enonce R14 compte les lignes où au moins un mot est sorti : de 2 à 50, il y en a 27
;@enonce (les multiples de 3, de 5 ou de 7, sans compter deux fois ceux de 15, 21 et 35).
;@attendu r14 == 27
;@attendu exit == 0

section .data
    fizz db "Fizz"
    buzz db "Buzz"
    bang db "Bang"
    saut db 10

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

; divisible(R12 par R13) → ZF armé si le reste est nul.
; RAX, RDX sont écrasés ; R12 et R13 sont préservés.
reste:
    mov rax, r12
    xor rdx, rdx
    div r13
    cmp rdx, 0
    ret

_start:
    xor r14, r14                ; lignes non vides
    mov r12, 2                  ; le nombre courant

.boucle:
    cmp r12, 50
    jg .fin                     ; le cours demandait 20 ; ici on va jusqu'à 50

    xor r15, r15                ; a-t-on écrit quelque chose sur cette ligne ?

    mov r13, 3
    call reste
    jne .test_5
    mov rdi, fizz
    mov rsi, 4
    call afficher
    mov r15, 1

.test_5:
    mov r13, 5
    call reste
    jne .test_7
    mov rdi, buzz
    mov rsi, 4
    call afficher
    mov r15, 1

.test_7:
    ; TODO : même schéma avec 7 et « bang »  (mets R13 à 7, appelle « reste »,
    ; TODO : saute à .ligne si le reste n'est pas nul, sinon affiche et R15 = 1)

.ligne:
    cmp r15, 0
    je .suivant                 ; rien à dire sur ce nombre
    inc r14
    mov rdi, saut
    mov rsi, 1
    call afficher

.suivant:
    inc r12
    jmp .boucle

.fin:
    mov rax, 60
    xor rdi, rdi
    syscall
