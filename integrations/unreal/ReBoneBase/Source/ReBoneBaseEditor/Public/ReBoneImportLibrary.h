// Editor helpers called by ReBoneImport.py to build Paper2D assets exactly.
#pragma once

#include "CoreMinimal.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "ReBoneImportLibrary.generated.h"

class UPaperFlipbook;
class UPaperSprite;
class UTexture2D;

UCLASS()
class REBONEBASEEDITOR_API UReBoneImportLibrary : public UBlueprintFunctionLibrary
{
	GENERATED_BODY()

public:
	/** Point a sprite at a texture region with a custom pivot in texture pixels and no sprite collision. */
	UFUNCTION(BlueprintCallable, Category = "ReBoneBase")
	static bool ConfigureSprite(UPaperSprite* Sprite, UTexture2D* Texture, FIntPoint Offset, FIntPoint Dimension, FVector2D TexturePivot, float PixelsPerUnit);

	/** Replace a flipbook's key frames; `Runs[i]` is how many 1/FramesPerSecond ticks `Sprites[i]` is held. */
	UFUNCTION(BlueprintCallable, Category = "ReBoneBase")
	static bool ConfigureFlipbook(UPaperFlipbook* Flipbook, const TArray<UPaperSprite*>& Sprites, const TArray<int32>& Runs, float FramesPerSecond);
};
