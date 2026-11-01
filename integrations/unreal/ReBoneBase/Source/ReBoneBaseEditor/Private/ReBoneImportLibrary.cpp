#include "ReBoneImportLibrary.h"

#include "Engine/Texture2D.h"
#include "PaperFlipbook.h"
#include "PaperSprite.h"
#include "SpriteEditorOnlyTypes.h"

bool UReBoneImportLibrary::ConfigureSprite(UPaperSprite* Sprite, UTexture2D* Texture, FIntPoint Offset, FIntPoint Dimension, FVector2D TexturePivot, float PixelsPerUnit)
{
	if (!Sprite || !Texture || Dimension.X <= 0 || Dimension.Y <= 0 || PixelsPerUnit <= 0.f)
	{
		return false;
	}
	Sprite->Modify();
	FSpriteAssetInitParameters Params;
	Params.Texture = Texture;
	Params.Offset = Offset;
	Params.Dimension = Dimension;
	Params.SetPixelsPerUnrealUnit(PixelsPerUnit);
	Sprite->InitializeSprite(Params, false);
	// Region boxes come from the package; sprite-derived physics would add a second, different shape.
	if (FByteProperty* Collision = FindFProperty<FByteProperty>(UPaperSprite::StaticClass(), TEXT("SpriteCollisionDomain")))
	{
		Collision->SetPropertyValue_InContainer(Sprite, static_cast<uint8>(ESpriteCollisionMode::None));
	}
	// Keep the exported pivot exactly; snapping would move fractional anchors by up to half a pixel.
	if (FBoolProperty* Snap = FindFProperty<FBoolProperty>(UPaperSprite::StaticClass(), TEXT("bSnapPivotToPixelGrid")))
	{
		Snap->SetPropertyValue_InContainer(Sprite, false);
	}
	Sprite->SetPivotMode(ESpritePivotMode::Custom, TexturePivot, true);
	Sprite->PostEditChange();
	Sprite->MarkPackageDirty();
	return true;
}

bool UReBoneImportLibrary::ConfigureFlipbook(UPaperFlipbook* Flipbook, const TArray<UPaperSprite*>& Sprites, const TArray<int32>& Runs, float FramesPerSecond)
{
	if (!Flipbook || Sprites.Num() != Runs.Num() || FramesPerSecond <= 0.f)
	{
		return false;
	}
	Flipbook->Modify();
	{
		FScopedFlipbookMutator Mutator(Flipbook);
		Mutator.FramesPerSecond = FramesPerSecond;
		Mutator.KeyFrames.Reset(Sprites.Num());
		for (int32 I = 0; I < Sprites.Num(); ++I)
		{
			FPaperFlipbookKeyFrame& Key = Mutator.KeyFrames.AddDefaulted_GetRef();
			Key.Sprite = Sprites[I];
			Key.FrameRun = FMath::Max(Runs[I], 1);
		}
	}
	Flipbook->PostEditChange();
	Flipbook->MarkPackageDirty();
	return true;
}
