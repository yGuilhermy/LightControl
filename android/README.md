# LightControl Android

App nativo Android 11 ou superior, em Java e Views/Canvas do sistema. Não usa WebView, servidor, nuvem, serviços do Google, SDK Tuya ou bibliotecas externas no aplicativo. O transporte Tuya 3.5 foi portado do projeto Rust e validado com a mesma fixture independente do TinyTuya.

## Instalar e conectar

1. Copie `dist\LightControl-Android.apk` para o celular e abra o arquivo para instalar. Permita a instalação por esse aplicativo quando o Android solicitar.
2. Deixe o celular na rede Wi-Fi que alcança a lâmpada.
3. No PC, no mesmo usuário Windows que configurou o LightControl, execute no PowerShell 7.2 ou superior:

   ```powershell
   .\scripts\export-android-key.ps1
   ```

   Crie uma senha de pelo menos 12 caracteres. O script lê a credencial DPAPI e produz `dist\device-transfer.lck`, cifrado com PBKDF2-SHA256 e AES-256-GCM. Não imprime ID ou chave. Se o arquivo já existir, escolha outra saída com `-OutputPath`.
4. Copie o arquivo `.lck` para o celular. No app, abra **Ajustes → Importar chave do PC…**, selecione o arquivo e informe a senha.
5. Como alternativa, informe IP, ID e `local_key` manualmente em Ajustes. A chave deve ter 16 bytes. O arquivo `device.dpapi` do Windows não é importável diretamente no Android.

A credencial fica cifrada no armazenamento privado, com uma chave gerada no Android Keystore. Não há credencial real embutida no APK. A chave Tuya precisa estar disponível transitoriamente em memória para negociar a sessão. O backup do aplicativo está desativado; reinstalar pode exigir nova importação.

Se o PC ou outro controlador ocupar a conexão da lâmpada, recolher o aplicativo Windows pode não liberá-la: encerre esse controlador e use **Reconectar** no Android. A descoberta escuta anúncios UDP nas portas 6666, 6667 e 7000 e valida a chave antes de salvar um novo IP. O IP manual funciona quando broadcasts não atravessam a rede.

## Device Control / Samsung

Depois de configurar a luz no app:

- **Samsung One UI:** abra o painel rápido, toque em **Controle de aparelhos** / **Device control**, abra a seleção de aplicativos/provedores, habilite **LightControl** e marque a luz. Os nomes e a localização variam com a versão da One UI.
- **Outros Android:** abra **Controles do dispositivo**, escolha adicionar controles e selecione **LightControl**.

O cartão oferece ligar/desligar e brilho. Manter o cartão pressionado abre o aplicativo. As ações usam o mesmo controlador LAN e só retornam sucesso após confirmação do dispositivo. Não é necessário manter a tela do LightControl aberta. O Android inicia o processo do provedor quando necessário.

O painel do sistema é desenhado pelo Android/Samsung com seus templates nativos. A interface do app usa a identidade visual do PC: mesmas cores, lâmpada, gradientes, paleta, páginas e ações. Em retrato, os cartões ficam em uma coluna com rolagem; a partir de 700 dp, usa duas colunas e medidas próximas às do desktop. Fontes do sistema, áreas de toque e diálogos seguem as APIs Android; não foi feita uma certificação de equivalência pixel a pixel entre plataformas.

## Recursos

- Energia, brilho, branco quente/frio relativo, HSV, código hexadecimal e favoritas.
- Ajustes com a luz apagada ficam preparados na sessão e são aplicados juntos ao ligar pelo app ou Device Control. Branco e cor preservam valores separados.
- Presets editáveis, incluindo Leitura, Descanso, Concentrar e Cinema; transição opcional de até 60 segundos.
- Cenas estáticas, alternadas e com transição, até oito cores, velocidade e brilho. Aurora, Oceano e Festa são as cenas padrão.
- Timer nativo da lâmpada de 1–1440 minutos; continua com o app fechado. Requer lâmpada ligada e suporte ao DP 26 confirmado. Desligar pelo app cancela o contador. Não há fallback de timer executado no celular nesta versão.
- Rotinas por horário e dias, com configuração de alarmes precisos quando autorizada. Exigem alcance local da lâmpada. O Android pode adiar execução em economia de bateria/Doze; atrasos superiores a dois minutos são descartados e não repetidos. Não se promete execução pontual em suspensão.
- Não perturbe após falta de energia aparece quando o firmware reporta o DP correspondente.

Presets, cenas, rotinas e favoritas são independentes dos dados do PC e da nuvem. A importação transfere a credencial, não a biblioteca pessoal de configurações. A primeira tela aparece sem aguardar a conexão. A animação tem limite de 20 quadros/s, só redesenha a lâmpada visível e respeita animações desativadas no Android. A sessão é liberada após cerca de 2,5 segundos sem consumidores; não há serviço permanente apenas para mantê-la aberta.

## Compilar

Requisitos: Windows, PowerShell 7.2+, JDK 17+ e acesso à internet para obter as ferramentas oficiais na primeira preparação.

```powershell
.\android\bootstrap.ps1
.\android\build.ps1
```

`build.ps1` usa `javac`, `aapt2`, D8, zipalign e apksigner do SDK. A saída é `dist\LightControl-Android.apk`, sem código de testes. Também existe um projeto Gradle para abrir no Android Studio (AGP 8.13, Gradle 8.13, compile/target SDK 36, min SDK 30).

O SDK isolado fica em `android/tools/sdk`; um SDK convencional pode ser passado por `-SdkRoot`. As ferramentas de desenvolvimento não são distribuídas no APK. O APK gerado usa uma assinatura local de desenvolvimento, armazenada em `android/tools/development-signing.p12`, e não habilita `debuggable`. Preserve a chave para atualizar uma instalação existente. Para outra assinatura:

```powershell
$env:LIGHTCONTROL_KEYSTORE_PASSWORD = 'senha-da-sua-chave'
.\android\build.ps1 -KeyStore 'caminho\release.p12' -KeyAlias 'seu-alias'
```

A permissão `INTERNET` é necessária para sockets LAN mesmo sem nuvem. O manifesto declara a permissão de rede local para evolução da plataforma; o pedido em runtime está condicionado ao Android e target SDK 37 ou superior. Esta compilação tem target 36.

## Validação e Integridade

A implementação conta com testes de integridade para criptografia Tuya 3.5, frames, modelo, cenas, IPv4 e sessão TCP simulada (incluindo fragmentação, frames agrupados e confirmações de estado). O transporte foi verificado para interoperabilidade com as especificações do protocolo Tuya LAN sem qualquer dependência de nuvem.
