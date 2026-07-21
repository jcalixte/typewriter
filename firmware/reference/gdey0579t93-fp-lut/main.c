#include "stm32f10x.h"
#include "delay.h"
#include "sys.h"
//EPD
#include "Display_EPD_W21_spi.h"
#include "Display_EPD_W21.h"
#include "Ap_29demo.h"	

//Tips//
/*
1.Flickering is normal when EPD is performing a full screen update to clear ghosting from the previous image so to ensure better clarity and legibility for the new image.
2.There will be no flicker when EPD performs a partial refresh.
3.Please make sue that EPD enters sleep mode when refresh is completed and always leave the sleep mode command. Otherwise, this may result in a reduced lifespan of EPD.
4.Please refrain from inserting EPD to the FPC socket or unplugging it when the MCU is being powered to prevent potential damage.)
5.Re-initialization is required for every full screen update.
6.When porting the program, set the BUSY pin to input mode and other pins to output mode.
*/

int	main(void)
{
	  unsigned char i,j;
		delay_init();	    	     //Delay function initialization
		NVIC_Configuration(); 	//Set NVIC interrupt grouping 2
    EPD_GPIO_Init();       //EPD GPIO  initialization
	while(1)
	{    

#if 0 //Full screen refresh, fast refresh, and partial refresh demostration.
			EPD_HW_Init_LUT(); //Full screen refresh initialization.
			EPD_WhiteScreen_White(); //Clear screen function.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(3); //Delay for 2s.
		 /************Full display(2s)*******************/
			EPD_HW_Init_LUT(); //Full screen refresh initialization.
			EPD_WhiteScreen_ALL(gImage_1); //To Display one image using full screen refresh.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(2); //Delay for 2s.	
			EPD_HW_Init_LUT(); //Full screen refresh initialization.
			EPD_WhiteScreen_ALL(gImage_2); //To Display one image using full screen refresh.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(2); //Delay for 2s.	
		
			EPD_HW_Init_LUT(); //Full screen refresh initialization.
			EPD_WhiteScreen_White(); //Clear screen function.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(5); //Delay for 2s.
		
			/************Fast refresh mode(1.5s)*******************/
			EPD_HW_Init_LUT_Fast(); //Full screen refresh initialization.
			EPD_WhiteScreen_ALL(gImage_1); //To Display one image using full screen refresh.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(2); //Delay for 2s.	
			EPD_HW_Init_LUT_Fast(); //Full screen refresh initialization.
			EPD_WhiteScreen_ALL(gImage_2); //To Display one image using full screen refresh.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(2); //Delay for 2s.		
			
			EPD_HW_Init_LUT_Fast(); //Full screen refresh initialization.
			EPD_WhiteScreen_White(); //Clear screen function.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(5); //Delay for 2s.		
		
		/*	EPD_HW_Init_Fast(); //Fast refresh initialization.
			EPD_WhiteScreen_ALL_Fast(gImage_1); //To display one image using fast refresh.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(2); //Delay for 2s.
			EPD_HW_Init_Fast(); //Fast refresh initialization.
			EPD_WhiteScreen_ALL_Fast(gImage_2); //To display the second image using fast refresh.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(2); //Delay for 2s.
*/
#endif	
	#if 1 //Partial refresh demostration.
	//Partial refresh demo support displaying a clock at 5 locations with 00:00.  If you need to perform partial refresh more than 5 locations, please use the feature of using partial refresh at the full screen demo.
	//After 5 partial refreshes, implement a full screen refresh to clear the ghosting caused by partial refreshes.
	//////////////////////Partial refresh time demo/////////////////////////////////////
			//EPD_HW_Init_LUT(); //Electronic paper initialization.	
			//EPD_SetRAMValue_BaseMap_LUT(gImage_basemap); //Please do not delete the background color function, otherwise it will cause unstable display during partial refresh.
			
			EPD_HW_Init(); //Electronic paper initialization.	
			EPD_SetRAMValue_BaseMap(gImage_basemap); //Please do not delete the background color function, otherwise it will cause unstable display during partial refresh.
			
	    EPD_HW_Init_Part();
//		Display_All_White1();
	    for(j=0;j<2;j++)
			for(i=0;i<10;i++)
  {
	      Epaper_Init1();
        Epaper_Partial();
			EPD_Dis_Part_myself_S_LUT(64,80,Num[1],         //x-A,y-A,DATA-A
													64+48,80,Num[0],         //x-B,y-B,DATA-B
													64+48*2,80,gImage_numdot,       //x-C,y-C,DATA-C
													64+48*3,80,Num[i],       //x-D,y-D,DATA-D
													64+48*4,80,Num[i],104,48);	 //x-D,y-D,DATA-D,Resolution 104*48				
   //delay_s(1);	//Delay for 2s. 
	}
		// OTP
		/*	    EPD_HW_Init_Part();
			for(j=0;j<2;j++)
			for(i=0;i<10;i++)
			{
			EPD_Dis_Part_myself_S(64,80,Num[1],         //x-A,y-A,DATA-A
													64+48,80,Num[0],         //x-B,y-B,DATA-B
													64+48*2,80,gImage_numdot,       //x-C,y-C,DATA-C
													64+48*3,80,Num[0],       //x-D,y-D,DATA-D
													64+48*4,80,Num[i],104,48);	 //x-D,y-D,DATA-D,Resolution 104*48												
       delay_s(2);	//Delay for 2s.
      }
	*/
		
			EPD_DeepSleep();  //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(30);	//Delay for 2s.

	    EPD_HW_Init_LUT(); //Full screen refresh initialization.
			EPD_WhiteScreen_White(); //Clear screen function.
			EPD_DeepSleep(); //Enter the sleep mode and please do not delete it, otherwise it will reduce the lifespan of the screen.
			delay_s(2); //Delay for 2s.
	#endif	
	


  while(1);	// The program stops here
	}
}	


